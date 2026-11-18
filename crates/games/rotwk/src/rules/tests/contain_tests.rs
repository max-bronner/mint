use super::*;

const DATA: &str = "\
GameData\n  MaxTunnelCapacity = 2\nEnd\n\
Locomotor TestFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object TestTower\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    KillPassengersOnDeath = Yes\n    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    PassengerBonePrefix = PassengerBone:ARROW_ KindOf:INFANTRY\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n    \
    NumberOfExitPaths = 2\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestRuin\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 1\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowEnemiesInside = Yes\n    AllowNeutralInside = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestKeep\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY -CAVALRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestVault\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    AllowEnemiesInside = No\n    AllowAlliesInside = No\n    AllowNeutralInside = No\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestFortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE GARRISONABLE_UNTIL_DESTROYED COMMANDCENTER\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Upgrade Upgrade_RingHolderTest\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
CommandSet TestRingBearerSet\n  1 = Buy_RingHolderTest\nEnd\n\
CommandButton Buy_RingHolderTest\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_RingHolderTest\nEnd\n\
Object TestRingBearer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  CommandSet = TestRingBearerSet\n  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Status\n    StatusToSet = HOLDING_THE_RING\n    \
    TriggeredBy = Upgrade_RingHolderTest\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestPit\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SlaughterHordeContain Tag_Pit\n    \
    ObjectStatusOfContained = UNSELECTABLE ENCLOSED\n    ContainMax = 99\n    \
    CashBackPercent = 200%\n    PassengerFilter = ANY +INFANTRY\n    \
    AllowOwnPlayerInsideOverride = Yes\n    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestTunnelA\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = TunnelContain Tag_Tunnel\n    \
    ObjectStatusOfContained = UNSELECTABLE ENCLOSED\n    ContainMax = 1\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
ChildObject TestTunnelB TestTunnelA\nEnd\n\
Object TestBoat\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Boat\n    \
    ObjectStatusOfContained = UNSELECTABLE\n    Slots = 2\n    \
    PassengerFilter = NONE +TestSoldier\n    AllowOwnPlayerInsideOverride = Yes\n    \
    InitialPayload = TestSoldier 2\n    TypeOneForWeaponState = INFANTRY\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestSoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:TestSoldier Position:X:12 Y:0 Position:X:-12 Y:0 \
Position:X:0 Y:12 Position:X:0 Y:-12\n  End\nEnd\n\
Locomotor TestLaggardFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  SpeedDamaged = 2%\nEnd\n\
Object TestLaggard\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = TestLaggardFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestStragglerHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:TestLaggard Position:X:-50 Y:0 Position:X:12 Y:0\n  End\nEnd\n\
Weapon TestJab\n  AttackRange = 100\n  DelayBetweenShots = 60000\n  HitPercentage = 100\n  \
  DamageNugget\n    Damage = 95\n    DamageType = PIERCE\n  End\nEnd\n\
Object TestPricker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestJab\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = TestBarracksSet\n  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\nEnd\n\
CommandSet TestBarracksSet\n  1 = Build_TestSoldierHorde\n  2 = Build_TestStragglerHorde\n  \
  3 = Build_TestComboHorde\n  4 = Build_TestRingBearerHorde\nEnd\n\
CommandButton Build_TestSoldierHorde\n  Command = UNIT_BUILD\n  Object = TestSoldierHorde\nEnd\n\
CommandButton Build_TestStragglerHorde\n  Command = UNIT_BUILD\n  Object = TestStragglerHorde\nEnd\n\
Object TestFlyer\n  Side = Men\n  KindOf = AIRCRAFT INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BountyValue = 200\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestKnight\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ToggleMountedSpecialAbilityUpdate Tag_Mount\n    \
    SpecialPowerTemplate = TestMountPower\n    UnpackTime = 1\n    PreparationTime = 1\n    \
    PersistentPrepTime = 1\n    PackTime = 1\n    TriggerInstantlyOnCreate = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Upgrade Upgrade_CanEnterAnything\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
CommandSet TestPrivilegedSet\n  1 = Buy_CanEnterAnything\nEnd\n\
CommandButton Buy_CanEnterAnything\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_CanEnterAnything\nEnd\n\
Object TestPrivileged\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  CommandSet = TestPrivilegedSet\n  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Status\n    StatusToSet = CAN_ENTER_ANYTHING\n    \
    TriggeredBy = Upgrade_CanEnterAnything\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor TestSwivel\n  Surfaces = GROUND\n  TurnTime = 2000\nEnd\n\
Weapon TestBow\n  AttackRange = 100\n  DelayBetweenShots = 500\n  CanFireWhileMoving = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon TestMuteBow\n  AttackRange = 100\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object TestMuteRider\n  Side = Men\n  KindOf = INFANTRY IMMOBILE UNATTACKABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestMuteBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    CanAttackWhileContained = Yes\n    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = TestSwivel\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestMuteMount\n  Side = Men\n  KindOf = SELECTABLE TRANSPORT\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Howdah\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE CAN_ATTACK\n    Slots = 1\n    \
    PassengerFilter = NONE +TestMuteRider\n    AllowOwnPlayerInsideOverride = Yes\n    \
    ForceOrientationContainer = No\n    InitialPayload = TestMuteRider 1\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Weapon TestSmash\n  AttackRange = 100\n  DelayBetweenShots = 60000\n  HitPercentage = 100\n  \
  DamageNugget\n    Damage = 920\n    DamageType = CRUSH\n  End\nEnd\n\
Object TestBasher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestSmash\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestRider\n  Side = Men\n  KindOf = INFANTRY IMMOBILE UNATTACKABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    CanAttackWhileContained = Yes\n    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = TestSwivel\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestCarrier\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Behavior = EvacuateDamage Tag_Evac\n    WeaponThatCausesEvacuation = TestSmash\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestBastion\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE GARRISON\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestComboHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE COMBO_HORDE\n  \
  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:TestSoldier Position:X:12 Y:0 Position:X:-12 Y:0\n  End\nEnd\n\
CommandButton Build_TestComboHorde\n  Command = UNIT_BUILD\n  Object = TestComboHorde\nEnd\n\
Object TestWagon\n  Side = Men\n  KindOf = SELECTABLE VEHICLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestMount\n  Side = Men\n  KindOf = SELECTABLE TRANSPORT\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Howdah\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE CAN_ATTACK\n    Slots = 1\n    \
    PassengerFilter = NONE +TestRider\n    AllowOwnPlayerInsideOverride = Yes\n    \
    ForceOrientationContainer = No\n    InitialPayload = TestRider 1\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Locomotor TestSiegeLoco\n  Surfaces = GROUND\n  Speed = 0\n  CrewPowered = Yes\n  \
  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object TestCrewMember\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestSiege\n  Side = Men\n  KindOf = SELECTABLE VEHICLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SiegeEngineContain Tag_Crew\n    \
    ObjectStatusOfContained = UNSELECTABLE\n    ObjectStatusOfCrew = UNATTACKABLE\n    CrewMax = 4\n    \
    CrewFilter = NONE +TestCrewMember\n    AllowOwnPlayerInsideOverride = Yes\n    \
    InitialCrew = TestCrewMember 2\n    SpeedPercentPerCrew = 40%\n  End\n  \
  LocomotorSet\n    Locomotor = TestSiegeLoco\n    Condition = SET_NORMAL\n    Speed = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Object TestSiegeNoCrew\n  Side = Men\n  KindOf = SELECTABLE VEHICLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = SiegeEngineContain Tag_Crew\n    \
    ObjectStatusOfContained = UNSELECTABLE\n    CrewMax = 4\n    \
    CrewFilter = NONE +TestCrewMember\n    AllowOwnPlayerInsideOverride = Yes\n    \
    SpeedPercentPerCrew = 40%\n  End\n  \
  LocomotorSet\n    Locomotor = TestSiegeLoco\n    Condition = SET_NORMAL\n    Speed = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Upgrade Upgrade_TestRing\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
CommandSet TestRingBearerSet\n  1 = Buy_TestRing\nEnd\n\
CommandButton Buy_TestRing\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_TestRing\nEnd\n\
Object TestRingBearer\n  Side = Men\n  KindOf = SELECTABLE\n  \
  CommandSet = TestRingBearerSet\n  BuildCost = 150\n  BuildTime = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Status\n    StatusToSet = HOLDING_THE_RING\n    \
    TriggeredBy = Upgrade_TestRing\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet TestRingBearerHordeSet\n  1 = Buy_TestRing\nEnd\n\
Object TestRingBearerHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  CommandSet = TestRingBearerHordeSet\n  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:TestRingBearer Position:X:12 Y:0 Position:X:-12 Y:0\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Status\n    StatusToSet = HOLDING_THE_RING\n    \
    TriggeredBy = Upgrade_TestRing\n  End\nEnd\n\
CommandButton Build_TestRingBearerHorde\n  Command = UNIT_BUILD\n  Object = TestRingBearerHorde\nEnd\n\
Upgrade Upgrade_TestRingPlayerBit\n  Type = PLAYER\nEnd\n\
Upgrade Upgrade_TestRingObjectBit\n  Type = OBJECT\nEnd\n\
Object TestWard\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestCitadel\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = CitadelSlaughterHordeContain Tag_Citadel\n    \
    ObjectStatusOfContained = UNSELECTABLE ENCLOSED\n    ContainMax = 99\n    \
    CashBackPercent = 200%\n    PassengerFilter = ANY +INFANTRY\n    \
    AllowOwnPlayerInsideOverride = Yes\n    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n    \
    ExitDelay = 10000\n    StatusForRingEntry = HOLDING_THE_RING\n    \
    ObjectToDestroyForRingEntry = ANY +TestWard\n    \
    UpgradeForRingEntry = Upgrade_TestRingPlayerBit Upgrade_TestRingObjectBit\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestCastle\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 4\n    \
    PassengerFilter = ANY +INFANTRY +VEHICLE\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestBay\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Bay\n    \
    ObjectStatusOfContained = UNSELECTABLE\n    Slots = 6\n    \
    PassengerFilter = ANY +INFANTRY +VEHICLE\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n\
Object TestCab\n  Side = Men\n  KindOf = SELECTABLE VEHICLE TRANSPORT\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Cab\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK\n    Slots = 1\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n";

struct Base {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Base {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "TestTower",
        "TestRuin",
        "TestKeep",
        "TestVault",
        "TestFortress",
        "TestPit",
        "TestTunnelA",
        "TestTunnelB",
        "TestBoat",
        "TestFlyer",
        "TestSoldier",
        "TestKnight",
        "TestPrivileged",
        "TestRingBearer",
        "TestBasher",
        "TestCarrier",
        "TestBastion",
        "TestWagon",
        "TestBarracks",
        "TestLaggard",
        "TestPricker",
        "TestMount",
        "TestMuteMount",
        "TestMuteRider",
        "TestCrewMember",
        "TestSiege",
        "TestSiegeNoCrew",
        "TestRingBearer",
        "TestRingBearerHorde",
        "TestWard",
        "TestCitadel",
        "TestCastle",
        "TestBay",
        "TestCab",
        "TestSoldierHorde",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(b: &Base, placed: &[(&str, f32, f32)]) -> (Simulation<RotwkRules>, Ids) {
    let owned: Vec<(&str, f32, f32, PlayerId)> = placed
        .iter()
        .map(|(name, x, y)| (*name, *x, *y, PLAYER))
        .collect();
    start_owned(b, &owned)
}

fn start_owned(b: &Base, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    start_perched(b, placed, Vec::new())
}

fn start_perched(
    b: &Base,
    placed: &[(&str, f32, f32, PlayerId)],
    perches: Vec<Vec<crate::perches::Perch>>,
) -> (Simulation<RotwkRules>, Ids) {
    let production = b.kinds.production(&b.data);
    let power_names = crate::powers::PowerNames::build(b.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&b.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = b.kinds.kind(name).unwrap();
            b.kinds
                .spawn(
                    &b.data.combat,
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
        .with_hordes(b.data.formations.tables().to_vec(), Vec::new())
        .with_players(2)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_upgrades(b.kinds.upgrades(&b.data), Arc::clone(&b.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_passenger_perches(perches)
        .with_max_tunnel_capacity(b.data.max_tunnel_capacity);
    let simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn of_kind(b: &Base, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Entity {
    let kind = b.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .unwrap()
}

fn order_in(simulation: &mut Simulation<RotwkRules>, soldier: Entity, container: Entity) {
    order_in_as(simulation, PLAYER, soldier, container);
}

fn order_in_as(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    soldier: Entity,
    container: Entity,
) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![soldier],
            },
        )
        .unwrap();
    simulation
        .submit(player, RotwkCommand::Enter { target: container })
        .unwrap();
    run(simulation, 60);
}

#[test]
fn a_soldier_ordered_into_a_tower_is_held_and_takes_the_declared_status() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTower", 300.0, 300.0), ("TestSoldier", 330.0, 300.0)],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in(&mut simulation, soldier, tower);

    let held = simulation.state().get(ids.passengers, tower).unwrap();
    assert_eq!(held.0, vec![soldier]);
    let status = simulation
        .state()
        .get(ids.contained, soldier)
        .unwrap()
        .status;
    assert!(status.has(super::super::contain::ContainStatus::UNSELECTABLE));
    assert!(status.has(super::super::contain::ContainStatus::CAN_ATTACK));
    assert!(status.has(super::super::contain::ContainStatus::ENCLOSED));
}

#[test]
fn an_entering_soldier_goes_in_by_the_door_not_the_wall_it_stands_at() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTower", 300.0, 300.0), ("TestSoldier", 272.0, 300.0)],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![soldier],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: tower })
        .unwrap();
    run(&mut simulation, 5);
    let walk = simulation.state().get(ids.entering, soldier).unwrap();
    assert!(!walk.through, "not yet through the door");

    run(&mut simulation, 300);
    assert!(simulation.state().contains(ids.contained, soldier));
}

#[test]
fn exiting_a_tower_returns_the_passenger_and_clears_its_status() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTower", 300.0, 300.0), ("TestSoldier", 330.0, 300.0)],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in(&mut simulation, soldier, tower);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![tower],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Exit { passenger: None })
        .unwrap();
    run(&mut simulation, 5);

    assert!(!simulation.state().contains(ids.contained, soldier));
    assert!(!simulation.state().contains(ids.passengers, tower));
}

#[test]
fn an_exit_container_slot_lets_only_its_own_passenger_out() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTower", 300.0, 300.0),
            ("TestSoldier", 330.0, 300.0),
            ("TestSoldier", 340.0, 310.0),
        ],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let kind = b.kinds.kind("TestSoldier").unwrap();
    let soldiers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    order_in(&mut simulation, soldiers[0], tower);
    order_in(&mut simulation, soldiers[1], tower);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::Exit {
                passenger: Some(soldiers[0]),
            },
        )
        .unwrap();
    run(&mut simulation, 5);

    assert!(!simulation.state().contains(ids.contained, soldiers[0]));
    assert!(simulation.state().contains(ids.contained, soldiers[1]));
    assert_eq!(
        simulation
            .state()
            .get(ids.passengers, tower)
            .map(|list| list.0.len()),
        Some(1)
    );
}

#[test]
fn every_member_of_a_horde_leaving_a_tower_comes_out_on_the_doors_side() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestTower", 300.0, 300.0)]);
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "TestSoldierHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 380.0,
                y: 300.0,
                facing: std::f32::consts::PI,
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    let store = simulation.state().store(ids.horde).unwrap();
    let horde = store.entities()[0];
    let members = store.get(horde).unwrap().members.clone();
    assert_eq!(members.len(), 4, "the fixture's premise");
    order_in(&mut simulation, members[0], tower);
    run(&mut simulation, 300);
    for member in &members {
        assert!(
            simulation.state().contains(ids.contained, *member),
            "member {} is not inside",
            member.index()
        );
    }

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![tower],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Exit { passenger: None })
        .unwrap();
    for tick in 0..120 {
        simulation.tick().unwrap();
        for member in &members {
            let at = simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position;
            assert!(
                at.x >= 299.0,
                "tick {tick}: member {} is at ({:.1}, {:.1}), behind the tower",
                member.index(),
                at.x,
                at.y
            );
        }
    }
    assert!(!simulation.state().contains(ids.contained, members[0]));
}

#[test]
fn a_garrisons_second_passenger_still_leaves_by_the_one_door() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTower", 300.0, 300.0),
            ("TestSoldier", 330.0, 300.0),
            ("TestSoldier", 340.0, 310.0),
        ],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let kind = b.kinds.kind("TestSoldier").unwrap();
    let soldiers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    order_in(&mut simulation, soldiers[0], tower);
    order_in(&mut simulation, soldiers[1], tower);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::Exit {
                passenger: Some(soldiers[1]),
            },
        )
        .unwrap();
    run(&mut simulation, 1);

    let goal = simulation
        .state()
        .get(ids.path, soldiers[1])
        .map(|path| path.goal);
    assert!(
        goal.is_some_and(|goal| goal.x > 300.0),
        "soldier 1 (list index 1 of 2 exit paths) should still walk out the east door, not {goal:?}"
    );
}

#[test]
fn a_passenger_stands_and_shoots_from_its_containers_next_passenger_bone() {
    let b = load();
    let bones = [[-18.0, 4.0, 66.0], [18.0, -4.0, 65.0]];
    let mut perches = vec![Vec::new(); b.kinds.len()];
    perches[usize::from(b.kinds.kind("TestTower").unwrap().0)] = bones
        .iter()
        .map(|&position| crate::perches::Perch {
            position,
            facing: Facing::EAST,
        })
        .collect();
    let (mut simulation, ids) = start_perched(
        &b,
        &[
            ("TestTower", 300.0, 300.0, PLAYER),
            ("TestSoldier", 330.0, 300.0, PLAYER),
            ("TestSoldier", 340.0, 310.0, PLAYER),
        ],
        perches,
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let kind = b.kinds.kind("TestSoldier").unwrap();
    let soldiers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    order_in(&mut simulation, soldiers[0], tower);
    order_in(&mut simulation, soldiers[1], tower);
    run(&mut simulation, 5);

    let world = simulation.state();
    let base = RotwkRules::feet(world, ids, tower);
    for (soldier, [x, y, z]) in soldiers.iter().zip(bones) {
        assert!(world.contains(ids.contained, *soldier));
        let at = world.get(ids.body, *soldier).unwrap().0.position;
        let feet = RotwkRules::feet(world, ids, *soldier);
        assert!(
            (at.x - (300.0 + x)).abs() < 1e-3 && (at.y - (300.0 + y)).abs() < 1e-3,
            "soldier {} stands at ({}, {}), not on its bone",
            soldier.index(),
            at.x,
            at.y
        );
        assert!(
            (feet - (base + z)).abs() < 1e-3,
            "feet {feet}, bone {}",
            base + z
        );
        let shape = world.get(ids.shape, *soldier).unwrap();
        let me = crate::combat::Foe {
            entity: *soldier,
            position: at,
            z: feet,
            radius: shape.radius,
            height: shape.height,
            is_structure: false,
            airborne: None,
            unattackable: false,
            not_autoacquirable: false,
            ai_kind_of: None,
        };
        let launch = simulation.rules().launch_of(world, *soldier, &me);
        assert_eq!((launch.x, launch.y, launch.z), (at.x, at.y, feet));
    }
    let seats: Vec<(Entity, Entity, usize, String)> = simulation
        .rules()
        .riders(world, ids)
        .into_iter()
        .map(|rider| (rider.rider, rider.container, rider.slot, rider.prefix))
        .collect();
    let mut expected: Vec<(Entity, Entity, usize, String)> = soldiers
        .iter()
        .enumerate()
        .map(|(slot, soldier)| (*soldier, tower, slot, "ARROW_".to_owned()))
        .collect();
    expected.sort_unstable_by_key(|seat| seat.0.index());
    assert_eq!(seats, expected);
}

#[test]
fn a_garrison_is_controlled_by_its_passengers_until_it_is_empty() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestRuin", 300.0, 300.0, other),
            ("TestSoldier", 330.0, 300.0, PLAYER),
        ],
    );
    let ruin = of_kind(&b, &simulation, ids, "TestRuin");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in(&mut simulation, soldier, ruin);
    let owner =
        |simulation: &Simulation<RotwkRules>| simulation.state().get(ids.owner, ruin).unwrap().0;
    assert_eq!(owner(&simulation), PLAYER);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ruin],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Exit { passenger: None })
        .unwrap();
    run(&mut simulation, 5);

    assert!(!simulation.state().contains(ids.contained, soldier));
    assert_eq!(owner(&simulation), other);
}

#[test]
fn a_tower_that_kills_its_passengers_takes_them_with_it() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTower", 300.0, 300.0), ("TestSoldier", 330.0, 300.0)],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in(&mut simulation, soldier, tower);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![tower],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    run(&mut simulation, 2);

    let health = simulation.state().get(ids.health, soldier).unwrap();
    assert!(health.0.is_dead(), "the passenger outlived its container");
}

#[test]
fn feeding_a_soldier_to_a_slaughter_pit_pays_its_bounty_value_not_its_build_cost() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestPit", 300.0, 300.0), ("TestSoldier", 330.0, 300.0)],
    );
    let pit = of_kind(&b, &simulation, ids, "TestPit");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    let before =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    order_in(&mut simulation, soldier, pit);
    let after =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(after - before, 400);
    assert!(
        !simulation.state().is_live(soldier) || simulation.state().contains(ids.doomed, soldier),
        "a slaughtered soldier is still alive"
    );
}

#[test]
fn a_horde_fed_to_a_slaughter_pit_pays_and_dies_member_by_member() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestPit", 300.0, 300.0)]);
    let pit = of_kind(&b, &simulation, ids, "TestPit");
    let horde = spawn_horde(&mut simulation, "TestSoldierHorde", 330.0, 300.0);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(
        members.len(),
        4,
        "TestSoldierHorde's own RankInfo declares 4 slots"
    );

    let before =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: pit })
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
        if members
            .iter()
            .any(|member| simulation.state().contains(ids.contained, *member))
        {
            break;
        }
    }
    let paid_for_one =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
            - before;
    assert_eq!(
        paid_for_one, 400,
        "one member's own bounty, not the whole horde's"
    );
    assert!(
        simulation.state().is_live(horde) && !simulation.state().contains(ids.doomed, horde),
        "the horde died at its first member's own entry"
    );

    run(&mut simulation, 400);
    let after =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount();
    assert_eq!(after - before, 1600, "all four members paid for by the end");
    assert!(
        !simulation.state().is_live(horde) || simulation.state().contains(ids.doomed, horde),
        "the horde outlived its last member"
    );
}

#[test]
fn a_tunnel_passenger_leaves_by_another_of_its_owners_tunnels() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTunnelA", 300.0, 300.0),
            ("TestTunnelB", 500.0, 300.0),
            ("TestSoldier", 330.0, 300.0),
        ],
    );
    let first = of_kind(&b, &simulation, ids, "TestTunnelA");
    let second = of_kind(&b, &simulation, ids, "TestTunnelB");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in(&mut simulation, soldier, first);
    assert!(simulation.state().contains(ids.contained, soldier));

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![second],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Exit { passenger: None })
        .unwrap();
    run(&mut simulation, 5);

    assert!(!simulation.state().contains(ids.contained, soldier));
    let at = simulation
        .state()
        .get(ids.body, soldier)
        .unwrap()
        .0
        .position;
    assert!(
        (at.x - 500.0).abs() < 100.0,
        "left by the second tunnel, not the first: {at:?}"
    );
}

#[test]
fn an_initial_payload_fills_the_container_and_raises_its_weapon_state() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestBoat", 300.0, 300.0)]);
    run(&mut simulation, 3);
    let boat = of_kind(&b, &simulation, ids, "TestBoat");

    let held = simulation.state().get(ids.passengers, boat).unwrap();
    assert_eq!(held.0.len(), 2, "InitialPayload = TestSoldier 2");
    let state = simulation.rules().upgrade_state_id().unwrap();
    let conditions = simulation.state().get(state, boat).unwrap();
    assert!(
        conditions
            .model_conditions
            .iter()
            .any(|name| name.as_ref() == "WEAPONSTATE_ONE"),
        "{:?}",
        conditions.model_conditions
    );
}

#[test]
fn a_rider_turns_on_its_mount_to_face_its_victim_and_then_shoots() {
    let b = load();
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestMount", 300.0, 300.0, PLAYER),
            ("TestSoldier", 300.0, 360.0, OTHER),
        ],
    );
    let mount = of_kind(&b, &simulation, ids, "TestMount");
    let victim = of_kind(&b, &simulation, ids, "TestSoldier");
    let health = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, victim)
            .unwrap()
            .0
            .current()
    };
    let facing = |simulation: &Simulation<RotwkRules>, entity: Entity| {
        simulation.state().get(ids.body, entity).unwrap().0.facing
    };
    for tick in 0..90 {
        simulation.tick().unwrap();
        if health(&simulation) < 100.0 {
            let rider = of_kind(&b, &simulation, ids, "TestRider");
            assert!(simulation.state().contains(ids.contained, rider));
            let off = facing(&simulation, rider)
                .delta_to(Facing::from_radians(core::f32::consts::FRAC_PI_2))
                .abs();
            assert!(
                off < 0.05,
                "shot on tick {tick}, {off} radians off its victim"
            );
            assert!(tick > 3, "shot on tick {tick}, before it could turn");
            assert_eq!(facing(&simulation, mount), Facing::EAST, "the mount turned");
            return;
        }
    }
    panic!("the rider never shot");
}

#[test]
fn a_flagged_riders_shot_does_not_wait_for_its_mount_to_stop() {
    assert!(rider_shoots_while_mount_walks("TestMount"));
}

#[test]
fn an_unflagged_rider_holds_fire_while_its_mount_moves() {
    assert!(!rider_shoots_while_mount_walks("TestMuteMount"));
}

fn rider_shoots_while_mount_walks(mount_name: &str) -> bool {
    let b = load();
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            (mount_name, 300.0, 300.0, PLAYER),
            ("TestSoldier", 300.0, 360.0, OTHER),
        ],
    );
    let mount = of_kind(&b, &simulation, ids, mount_name);
    let victim = of_kind(&b, &simulation, ids, "TestSoldier");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mount],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let health = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, victim)
            .unwrap()
            .0
            .current()
    };
    for _ in 0..90 {
        simulation.tick().unwrap();
        if health(&simulation) < 100.0 {
            let pos = simulation.state().get(ids.body, mount).unwrap().0.position;
            assert!(
                pos.x > 300.5,
                "the mount never got moving before the shot landed: {pos:?}"
            );
            return true;
        }
    }
    let pos = simulation.state().get(ids.body, mount).unwrap().0.position;
    assert!(
        pos.x > 300.5,
        "the mount never moved, so holding fire proves nothing: {pos:?}"
    );
    false
}

#[test]
fn an_initial_crew_fills_a_siege_engine_and_powers_its_locomotor() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestSiege", 300.0, 300.0)]);
    run(&mut simulation, 3);
    let siege = of_kind(&b, &simulation, ids, "TestSiege");

    let held = simulation.state().get(ids.passengers, siege).unwrap();
    assert_eq!(held.0.len(), 2, "InitialCrew = TestCrewMember 2");

    let factor = simulation
        .rules()
        .crew_speed_factor(simulation.state(), ids, siege);
    assert!(
        (factor - 0.8).abs() < 1e-6,
        "2 crew x SpeedPercentPerCrew 40% = 0.8, got {factor}"
    );
}

#[test]
fn a_siege_engine_with_no_initial_crew_spawns_nothing() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestSiegeNoCrew", 300.0, 300.0)]);
    run(&mut simulation, 3);
    let siege = of_kind(&b, &simulation, ids, "TestSiegeNoCrew");

    assert!(
        simulation.state().get(ids.passengers, siege).is_none(),
        "no InitialCrew means no passengers"
    );
}

#[test]
fn a_tunnel_networks_capacity_is_gamedatas_max_tunnel_capacity() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTunnelA", 300.0, 300.0),
            ("TestTunnelB", 500.0, 300.0),
            ("TestSoldier", 330.0, 300.0),
            ("TestSoldier", 530.0, 300.0),
        ],
    );
    let first = of_kind(&b, &simulation, ids, "TestTunnelA");
    let second = of_kind(&b, &simulation, ids, "TestTunnelB");
    let soldiers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| {
            **k == b.kinds.kind("TestSoldier").unwrap() && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(soldiers.len(), 2);
    order_in(&mut simulation, soldiers[0], first);
    order_in(&mut simulation, soldiers[1], second);

    assert!(simulation.state().contains(ids.contained, soldiers[0]));
    assert!(
        simulation.state().contains(ids.contained, soldiers[1]),
        "network capacity 2 (GameData) should admit both, not either tunnel's own ContainMax 1"
    );
}

#[test]
fn a_tunnel_refuses_an_aircraft() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTunnelA", 300.0, 300.0), ("TestFlyer", 330.0, 300.0)],
    );
    let tunnel = of_kind(&b, &simulation, ids, "TestTunnelA");
    let flyer = of_kind(&b, &simulation, ids, "TestFlyer");
    order_in(&mut simulation, flyer, tunnel);

    assert!(
        !simulation.state().contains(ids.contained, flyer),
        "an AIRCRAFT walker is refused by a tunnel"
    );
}

#[test]
fn a_filter_excluding_cavalry_refuses_a_mounted_walker() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestKeep", 300.0, 300.0), ("TestKnight", 330.0, 300.0)],
    );
    run(&mut simulation, 3);
    let keep = of_kind(&b, &simulation, ids, "TestKeep");
    let knight = of_kind(&b, &simulation, ids, "TestKnight");
    let state_id = simulation.rules().upgrade_state_id().unwrap();
    assert!(
        simulation
            .state()
            .get(state_id, knight)
            .is_some_and(|state| state
                .model_conditions
                .iter()
                .any(|c| c.as_ref() == "MOUNTED")),
        "TriggerInstantlyOnCreate must set MOUNTED at spawn"
    );

    order_in(&mut simulation, knight, keep);

    assert!(
        !simulation.state().contains(ids.contained, knight),
        "PassengerFilter excludes CAVALRY: a MOUNTED walker is refused"
    );
}

#[test]
fn a_can_enter_anything_status_bypasses_the_relationship_refusal() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestVault", 300.0, 300.0, other),
            ("TestSoldier", 330.0, 300.0, PLAYER),
            ("TestPrivileged", 330.0, 340.0, PLAYER),
        ],
    );
    let vault = of_kind(&b, &simulation, ids, "TestVault");
    let refused = of_kind(&b, &simulation, ids, "TestSoldier");
    let admitted = of_kind(&b, &simulation, ids, "TestPrivileged");
    research(&mut simulation, admitted, 0);

    order_in(&mut simulation, refused, vault);
    order_in(&mut simulation, admitted, vault);

    assert!(
        !simulation.state().contains(ids.contained, refused),
        "AllowEnemiesInside = No refuses an ordinary enemy"
    );
    assert!(
        simulation.state().contains(ids.contained, admitted),
        "CAN_ENTER_ANYTHING bypasses the relationship refusal"
    );
}

#[test]
fn ring_delivery_target_needs_a_commandcenter_and_a_ring_bearer_selected() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestFortress", 300.0, 300.0, other),
            ("TestVault", 400.0, 300.0, other),
            ("TestRingBearer", 330.0, 300.0, PLAYER),
            ("TestSoldier", 330.0, 340.0, PLAYER),
        ],
    );
    let fortress = of_kind(&b, &simulation, ids, "TestFortress");
    let vault = of_kind(&b, &simulation, ids, "TestVault");
    let bearer = of_kind(&b, &simulation, ids, "TestRingBearer");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    research(&mut simulation, bearer, 0);

    let rules = simulation.rules();
    let world = simulation.state();
    assert!(
        rules.ring_delivery_target(world, fortress, &[bearer]),
        "a COMMANDCENTER with the bearer selected is the delivery target"
    );
    assert!(
        !rules.ring_delivery_target(world, vault, &[bearer]),
        "a contain that is not COMMANDCENTER is never a delivery target"
    );
    assert!(
        !rules.ring_delivery_target(world, fortress, &[soldier]),
        "a selection with no ring bearer is not a delivery target"
    );
}

fn research(simulation: &mut Simulation<RotwkRules>, unit: Entity, button: u8) {
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
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button })
        .unwrap();
    run(simulation, 35);
}

#[test]
fn a_reallydamaged_garrison_refuses_entry() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestKeep", 300.0, 300.0, other),
            ("TestBasher", 330.0, 300.0, PLAYER),
            ("TestSoldier", 230.0, 300.0, other),
        ],
    );
    let keep = of_kind(&b, &simulation, ids, "TestKeep");
    let basher = of_kind(&b, &simulation, ids, "TestBasher");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![basher],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: keep })
        .unwrap();
    run(&mut simulation, 60);
    let health = simulation
        .state()
        .get(ids.health, keep)
        .unwrap()
        .0
        .current();
    assert!(
        health > 0.0 && health <= 100.0,
        "one 920-damage hit on a 1000-health keep should leave it REALLYDAMAGED (<=10%), not destroyed: {health}"
    );

    order_in_as(&mut simulation, other, soldier, keep);

    assert!(
        !simulation.state().contains(ids.contained, soldier),
        "a REALLYDAMAGED garrison refuses entry"
    );
}

#[test]
fn a_garrisonable_until_destroyed_structure_admits_while_reallydamaged() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestFortress", 300.0, 300.0, other),
            ("TestBasher", 330.0, 300.0, PLAYER),
            ("TestSoldier", 230.0, 300.0, other),
        ],
    );
    let fortress = of_kind(&b, &simulation, ids, "TestFortress");
    let basher = of_kind(&b, &simulation, ids, "TestBasher");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![basher],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: fortress })
        .unwrap();
    run(&mut simulation, 60);
    let health = simulation
        .state()
        .get(ids.health, fortress)
        .unwrap()
        .0
        .current();
    assert!(
        health > 0.0 && health <= 100.0,
        "one 920-damage hit on a 1000-health fortress should leave it REALLYDAMAGED: {health}"
    );

    order_in_as(&mut simulation, other, soldier, fortress);
    run(&mut simulation, 30);

    assert!(
        simulation.state().contains(ids.contained, soldier),
        "GARRISONABLE_UNTIL_DESTROYED waives the REALLYDAMAGED refusal"
    );
}

#[test]
fn a_weapon_named_by_evacuatedamage_empties_the_garrison() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestCarrier", 300.0, 300.0, other),
            ("TestBasher", 230.0, 300.0, PLAYER),
            ("TestSoldier", 305.0, 300.0, other),
        ],
    );
    let carrier = of_kind(&b, &simulation, ids, "TestCarrier");
    let basher = of_kind(&b, &simulation, ids, "TestBasher");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");
    order_in_as(&mut simulation, other, soldier, carrier);
    assert!(simulation.state().contains(ids.contained, soldier));

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![basher],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: carrier })
        .unwrap();
    run(&mut simulation, 60);

    assert!(
        !simulation.state().contains(ids.contained, soldier),
        "a hit from WeaponThatCausesEvacuation empties the garrison"
    );
}

fn spawn_horde(simulation: &mut Simulation<RotwkRules>, label: &str, x: f32, y: f32) -> Entity {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == label)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y,
                facing: 0.0,
            },
        )
        .unwrap();
    run(simulation, 2);
    simulation
        .state()
        .store(simulation.rules().ids().unwrap().horde)
        .unwrap()
        .entities()
        .last()
        .copied()
        .unwrap()
}

#[test]
fn a_mobile_containers_move_is_refused_while_a_horde_boards() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestWagon", 300.0, 300.0)]);
    let wagon = of_kind(&b, &simulation, ids, "TestWagon");
    let horde = spawn_horde(&mut simulation, "TestSoldierHorde", 60.0, 60.0);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: wagon })
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        simulation.state().contains(ids.entering, horde),
        "the horde should be admitted and on its way in"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![wagon],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 5);

    let pos = simulation.state().get(ids.body, wagon).unwrap().0.position;
    assert!(
        (pos.x - 300.0).abs() < 1.0 && (pos.y - 300.0).abs() < 1.0,
        "the wagon must not move while the horde boards: {pos:?}"
    );
}

#[test]
fn an_entering_order_with_no_path_to_the_door_is_dropped_after_its_grace_period() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestCarrier", 9000.0, 0.0), ("TestSoldier", 20.0, 20.0)],
    );
    let carrier = of_kind(&b, &simulation, ids, "TestCarrier");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![soldier],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: carrier })
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        simulation.state().contains(ids.entering, soldier),
        "the order should still be admitted and recorded"
    );
    assert!(
        !simulation.state().contains(ids.path, soldier),
        "the door is off the passability grid, so no path should ever be found"
    );

    run(&mut simulation, 35);
    assert!(
        !simulation.state().contains(ids.entering, soldier),
        "a mover that can never reach the door must have its order dropped, not held forever"
    );
    assert!(
        !simulation.state().contains(ids.contained, soldier),
        "it was never admitted - it must not end up inside either"
    );
}

#[test]
fn two_hordes_ordered_into_one_tower_together_both_get_in() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestTower", 300.0, 300.0)]);
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let horde_a = spawn_horde(&mut simulation, "TestSoldierHorde", 60.0, 60.0);
    let horde_b = spawn_horde(&mut simulation, "TestSoldierHorde", 60.0, 100.0);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde_a, horde_b],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: tower })
        .unwrap();
    run(&mut simulation, 400);

    assert!(
        simulation.state().contains(ids.contained, horde_a),
        "the first horde must get in"
    );
    assert!(
        simulation.state().contains(ids.contained, horde_b),
        "the second horde's group place must not be mistaken for a failed approach"
    );
}

#[test]
fn two_loose_soldiers_ordered_into_one_tower_together_both_get_in() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTower", 300.0, 300.0),
            ("TestSoldier", 0.0, 0.0),
            ("TestSoldier", 0.0, 60.0),
        ],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let kind = b.kinds.kind("TestSoldier").unwrap();
    let soldiers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(soldiers.len(), 2);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: soldiers.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: tower })
        .unwrap();
    run(&mut simulation, 400);

    for soldier in soldiers {
        assert!(
            simulation.state().contains(ids.contained, soldier),
            "both loose soldiers must get in, not just the one nearest the door"
        );
    }
}

#[test]
fn a_mobile_containers_hold_lasts_through_the_whole_boarding_not_just_the_approach() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestWagon", 300.0, 300.0)]);
    let wagon = of_kind(&b, &simulation, ids, "TestWagon");
    let horde = spawn_horde(&mut simulation, "TestSoldierHorde", 60.0, 60.0);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: wagon })
        .unwrap();

    let mut mid_stream = false;
    for _ in 0..600 {
        run(&mut simulation, 1);
        let horde_in = simulation.state().contains(ids.contained, horde);
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .clone();
        let any_in = members
            .iter()
            .any(|member| simulation.state().contains(ids.contained, *member));
        let all_in = members
            .iter()
            .all(|member| simulation.state().contains(ids.contained, *member));
        if horde_in && any_in && !all_in {
            mid_stream = true;
            break;
        }
        if horde_in && all_in {
            break;
        }
    }
    assert!(
        mid_stream,
        "the test must catch the horde with the group absorbed but a member still streaming in"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![wagon],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 5);
    let pos = simulation.state().get(ids.body, wagon).unwrap().0.position;
    assert!(
        (pos.x - 300.0).abs() < 1.0 && (pos.y - 300.0).abs() < 1.0,
        "the wagon must not move while any member is still streaming in: {pos:?}"
    );

    run(&mut simulation, 200);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert!(
        members
            .iter()
            .all(|member| simulation.state().contains(ids.contained, *member)),
        "the whole horde should finish boarding well within this budget"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![wagon],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 5);
    let pos = simulation.state().get(ids.body, wagon).unwrap().0.position;
    assert!(
        pos.x > 301.0 || pos.y > 301.0,
        "once boarding ends the wagon must be free to move: {pos:?}"
    );
}

#[test]
fn a_can_enter_anything_unit_is_still_refused_by_a_reallydamaged_garrison() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestKeep", 300.0, 300.0, other),
            ("TestBasher", 330.0, 300.0, PLAYER),
            ("TestPrivileged", 230.0, 300.0, other),
        ],
    );
    let keep = of_kind(&b, &simulation, ids, "TestKeep");
    let basher = of_kind(&b, &simulation, ids, "TestBasher");
    let privileged = of_kind(&b, &simulation, ids, "TestPrivileged");

    simulation
        .submit(
            other,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![privileged],
            },
        )
        .unwrap();
    simulation
        .submit(other, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    run(&mut simulation, 35);
    let status_id = simulation.rules().upgrade_state_id().unwrap();
    assert!(
        simulation
            .state()
            .get(status_id, privileged)
            .is_some_and(|state| state
                .status_bits
                .iter()
                .any(|s| s.eq_ignore_ascii_case("CAN_ENTER_ANYTHING"))),
        "the upgrade must have granted CAN_ENTER_ANYTHING"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![basher],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: keep })
        .unwrap();
    run(&mut simulation, 60);
    let health = simulation
        .state()
        .get(ids.health, keep)
        .unwrap()
        .0
        .current();
    assert!(
        health > 0.0 && health <= 100.0,
        "one 920-damage hit on a 1000-health keep should leave it REALLYDAMAGED: {health}"
    );

    order_in_as(&mut simulation, other, privileged, keep);
    run(&mut simulation, 30);

    assert!(
        !simulation.state().contains(ids.contained, privileged),
        "CAN_ENTER_ANYTHING bypasses relationship and capacity, not a REALLYDAMAGED garrison"
    );
}

#[test]
fn a_combo_horde_only_enters_garrison_or_cash_producer_containers() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestBastion", 80.0, 0.0), ("TestKeep", 80.0, 150.0)]);
    let bastion = of_kind(&b, &simulation, ids, "TestBastion");
    let keep = of_kind(&b, &simulation, ids, "TestKeep");
    let horde = spawn_horde(&mut simulation, "TestComboHorde", 60.0, 60.0);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: keep })
        .unwrap();
    run(&mut simulation, 400);
    assert!(
        !simulation.state().contains(ids.contained, horde),
        "a COMBO_HORDE walker must be refused by a non-GARRISON, non-FS_CASH_PRODUCER container"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: bastion })
        .unwrap();
    run(&mut simulation, 400);
    assert!(
        simulation.state().contains(ids.contained, horde),
        "a COMBO_HORDE walker is admitted by a GARRISON container"
    );
}

#[test]
fn a_crippled_horde_member_is_pulled_in_by_the_straggler_clock() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestTower", 300.0, 300.0, PLAYER),
            ("TestPricker", -50.0, 30.0, other),
        ],
    );
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let pricker = of_kind(&b, &simulation, ids, "TestPricker");
    let horde = spawn_horde(&mut simulation, "TestStragglerHorde", 60.0, 60.0);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(members.len(), 2);
    let crippled = members[0];

    simulation
        .submit(
            other,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![pricker],
            },
        )
        .unwrap();
    simulation
        .submit(other, RotwkCommand::AttackObject { target: crippled })
        .unwrap();
    run(&mut simulation, 20);
    let health = simulation
        .state()
        .get(ids.health, crippled)
        .unwrap()
        .0
        .current();
    assert!(
        health > 0.0 && health <= 10.0,
        "one 95-damage hit on 100 health should leave it at or below the REALLYDAMAGED rung \
         `SpeedDamaged` reads: {health}"
    );
    let steady = members.iter().find(|m| **m != crippled).copied().unwrap();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: tower })
        .unwrap();
    run(&mut simulation, 400);

    assert!(
        simulation.state().contains(ids.contained, steady),
        "the healthy member should walk in on its own"
    );
    assert!(
        simulation.state().contains(ids.contained, crippled),
        "the crippled member, still far short of the door on its own legs, must be pulled in by the straggler clock"
    );
}

#[test]
fn an_enter_order_voices_the_enter_not_the_walk() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestTower", 300.0, 300.0)]);
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let horde = spawn_horde(&mut simulation, "TestSoldierHorde", 60.0, 60.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: tower })
        .unwrap();
    run(&mut simulation, 1);
    let frame = Frame::new(simulation.frame().get() - 1);
    let events = simulation
        .rules()
        .voice_events(simulation.state(), frame)
        .to_vec();
    assert!(events.iter().any(|event| {
        event.kind == crate::voice::VoiceKind::Enter && event.target == Some(tower)
    }));
    assert!(events
        .iter()
        .all(|event| event.kind != crate::voice::VoiceKind::Move));
}

#[test]
fn a_ring_bearer_delivers_to_its_own_citadel_and_leaves_again() {
    let b = load();
    let other = PlayerId::new(1);
    let (mut simulation, ids) = start_owned(
        &b,
        &[
            ("TestCitadel", 300.0, 300.0, PLAYER),
            ("TestWard", 300.0, 330.0, other),
            ("TestWard", 300.0, 1300.0, PLAYER),
            ("TestRingBearer", 340.0, 300.0, PLAYER),
        ],
    );
    let citadel = of_kind(&b, &simulation, ids, "TestCitadel");
    let bearer = of_kind(&b, &simulation, ids, "TestRingBearer");
    let wards: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, kind)| **kind == b.kinds.kind("TestWard").unwrap())
        .map(|(entity, _)| entity)
        .collect();
    let near_ward = *wards
        .iter()
        .find(|entity| {
            simulation
                .state()
                .get(ids.owner, **entity)
                .is_some_and(|held| held.0 == other)
        })
        .unwrap();
    let far_ward = *wards.iter().find(|entity| **entity != near_ward).unwrap();

    research(&mut simulation, bearer, 0);
    let state_id = simulation.rules().upgrade_state_id().unwrap();
    assert!(
        simulation
            .state()
            .get(state_id, bearer)
            .is_some_and(|state| state
                .status_bits
                .iter()
                .any(|bit| bit.eq_ignore_ascii_case("HOLDING_THE_RING"))),
        "the bearer must hold the Ring status before entry is attempted"
    );

    order_in(&mut simulation, bearer, citadel);

    assert!(
        !simulation.state().is_live(near_ward),
        "the near ward, owned by another player, must be destroyed on delivery"
    );
    assert!(
        simulation.state().is_live(far_ward),
        "only the nearest ObjectToDestroyForRingEntry match is destroyed"
    );
    assert!(
        !simulation
            .state()
            .get(state_id, bearer)
            .is_some_and(|state| state
                .status_bits
                .iter()
                .any(|bit| bit.eq_ignore_ascii_case("HOLDING_THE_RING"))),
        "the status must be cleared on delivery"
    );
    let player_bit = simulation
        .rules()
        .upgrade_bits
        .bit("Upgrade_TestRingPlayerBit")
        .unwrap();
    let object_bit = simulation
        .rules()
        .upgrade_bits
        .bit("Upgrade_TestRingObjectBit")
        .unwrap();
    let player_entity =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(
        simulation
            .state()
            .get(ids.player_upgrades, player_entity)
            .is_some_and(|held| held.0.contains(player_bit)),
        "UpgradeForRingEntry's PLAYER bit must reach the owner"
    );
    assert!(
        simulation
            .state()
            .get(ids.object_upgrades, citadel)
            .is_some_and(|held| held.0.contains(object_bit)),
        "UpgradeForRingEntry's OBJECT bit must reach the citadel"
    );

    assert!(
        !simulation.state().contains(ids.contained, bearer),
        "the carrier delivers and leaves again within the same tick, not staying inside"
    );
}

#[test]
fn a_ring_bearing_horde_delivers_to_its_own_citadel() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestCitadel", 300.0, 300.0)]);
    let citadel = of_kind(&b, &simulation, ids, "TestCitadel");
    let horde = spawn_horde(&mut simulation, "TestRingBearerHorde", 60.0, 60.0);
    research(&mut simulation, horde, 0);

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: citadel })
        .unwrap();
    run(&mut simulation, 400);

    let object_bit = simulation
        .rules()
        .upgrade_bits
        .bit("Upgrade_TestRingObjectBit")
        .unwrap();
    assert!(
        simulation
            .state()
            .get(ids.object_upgrades, citadel)
            .is_some_and(|held| held.0.contains(object_bit)),
        "a ring-bearing horde must be admitted through the second door, same as a lone unit"
    );
}

#[test]
fn a_soldier_boarding_a_moving_wagon_tracks_its_door_once_it_drives_off() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestWagon", 400.0, 480.0), ("TestSoldier", 50.0, 480.0)],
    );
    let wagon = of_kind(&b, &simulation, ids, "TestWagon");
    let soldier = of_kind(&b, &simulation, ids, "TestSoldier");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![soldier],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::Enter { target: wagon })
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        simulation.state().contains(ids.entering, soldier),
        "the soldier should be admitted and on its way in"
    );
    let original_goal = simulation.state().get(ids.path, soldier).unwrap().goal;

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![wagon],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 480.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 310);

    let wagon_pos = simulation.state().get(ids.body, wagon).unwrap().0.position;
    assert!(
        wagon_pos.x > 450.0,
        "the wagon must actually have driven off: {wagon_pos:?}"
    );
    let current_goal = simulation.state().get(ids.path, soldier).unwrap().goal;
    assert!(
        current_goal.x - original_goal.x > 50.0,
        "the soldier's goal must have followed the door east: was {original_goal:?}, now \
         {current_goal:?}"
    );
    assert!(
        (current_goal.x - (wagon_pos.x + 30.0)).abs() < 30.0,
        "the goal should sit at the wagon's current door, not a stale one: goal {current_goal:?}, \
         wagon {wagon_pos:?}"
    );
}

#[test]
fn the_boarding_replan_interval_is_fifty_one_retail_frames() {
    assert_eq!(crate::rules::movement::BOARDING_REPLAN_FRAMES, 306);
}

#[test]
fn a_transport_whose_passengers_may_fire_and_holds_one_is_able_to_attack() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestMuteMount", 300.0, 300.0)]);
    run(&mut simulation, 3);
    let mount = of_kind(&b, &simulation, ids, "TestMuteMount");
    assert!(simulation
        .rules()
        .object_able_to_attack(simulation.state(), ids, mount));
}

#[test]
fn a_passenger_is_able_to_attack_only_where_its_container_lets_it_fire() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestTunnelA", 300.0, 300.0),
            ("TestTower", 400.0, 300.0),
            ("TestBasher", 330.0, 300.0),
            ("TestBasher", 430.0, 300.0),
        ],
    );
    let tunnel = of_kind(&b, &simulation, ids, "TestTunnelA");
    let tower = of_kind(&b, &simulation, ids, "TestTower");
    let bashers: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == b.kinds.kind("TestBasher").unwrap())
        .map(|(entity, _)| entity)
        .collect();
    let able = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .rules()
            .object_able_to_attack(simulation.state(), ids, entity)
    };
    assert!(able(&simulation, bashers[0]) && able(&simulation, bashers[1]));
    order_in(&mut simulation, bashers[0], tunnel);
    order_in(&mut simulation, bashers[1], tower);
    assert!(simulation.state().contains(ids.contained, bashers[0]));
    assert!(simulation.state().contains(ids.contained, bashers[1]));
    assert!(!able(&simulation, bashers[0]));
    assert!(able(&simulation, bashers[1]));
}

#[test]
fn a_tunnel_lets_passengers_fire_whatever_its_status_list_says() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestTunnelA", 300.0, 300.0), ("TestBasher", 330.0, 300.0)],
    );
    let tunnel = of_kind(&b, &simulation, ids, "TestTunnelA");
    let basher = of_kind(&b, &simulation, ids, "TestBasher");
    let able = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .object_able_to_attack(simulation.state(), ids, tunnel)
    };
    assert!(!able(&simulation), "empty: nobody to fire");
    order_in(&mut simulation, basher, tunnel);
    assert!(simulation.state().contains(ids.contained, basher));
    assert!(able(&simulation));
}

#[test]
fn a_low_passenger_may_not_fire_towards_its_container() {
    use crate::rules::group_move::refuses_by_height;
    let to_container = Vec2::new(10.0, 0.0);
    assert!(refuses_by_height(
        80.0,
        10.0,
        to_container,
        Vec2::new(50.0, 3.0)
    ));
    assert!(refuses_by_height(
        80.0,
        10.0,
        to_container,
        Vec2::new(0.0, 40.0)
    ));
    assert!(!refuses_by_height(
        80.0,
        10.0,
        to_container,
        Vec2::new(-50.0, 3.0)
    ));
    assert!(!refuses_by_height(
        80.0,
        90.0,
        to_container,
        Vec2::new(50.0, 3.0)
    ));
}

#[test]
fn a_siege_engines_crew_take_the_crew_status_not_the_contained_one() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("TestSiege", 300.0, 300.0)]);
    run(&mut simulation, 3);
    let siege = of_kind(&b, &simulation, ids, "TestSiege");
    let crew = simulation.state().get(ids.passengers, siege).unwrap().0[0];
    let held = simulation.state().get(ids.contained, crew).unwrap();
    assert!(held
        .status
        .has(crate::rules::contain::ContainStatus::UNATTACKABLE));
    assert!(!held
        .status
        .has(crate::rules::contain::ContainStatus::UNSELECTABLE));
}

#[test]
fn a_horde_lets_its_members_fire_as_the_container_holding_it_does() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[("TestCastle", 300.0, 300.0), ("TestBay", 500.0, 300.0)],
    );
    let castle = of_kind(&b, &simulation, ids, "TestCastle");
    let bay = of_kind(&b, &simulation, ids, "TestBay");
    let fires = |simulation: &Simulation<RotwkRules>, who| {
        simulation
            .rules()
            .lets_passengers_fire(simulation.state(), who)
    };
    let loose = spawn_horde(&mut simulation, "TestSoldierHorde", 300.0, 380.0);
    let housed = spawn_horde(&mut simulation, "TestSoldierHorde", 500.0, 380.0);
    assert!(!fires(&simulation, loose), "a horde held by nothing");
    order_in(&mut simulation, housed, bay);
    run(&mut simulation, 300);
    assert!(simulation.state().contains(ids.contained, housed));
    assert!(
        !fires(&simulation, housed),
        "a bay's status lacks CAN_ATTACK"
    );
    order_in(&mut simulation, loose, castle);
    run(&mut simulation, 300);
    assert!(simulation.state().contains(ids.contained, loose));
    assert!(
        fires(&simulation, loose),
        "a castle lets its passengers fire"
    );
}

#[test]
fn an_open_container_that_is_itself_held_asks_its_own_container() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("TestCastle", 300.0, 300.0),
            ("TestBay", 500.0, 300.0),
            ("TestCab", 300.0, 380.0),
            ("TestCab", 500.0, 380.0),
        ],
    );
    let all = |simulation: &Simulation<RotwkRules>, name: &str| -> Vec<Entity> {
        let kind = b.kinds.kind(name).unwrap();
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(_, k)| **k == kind)
            .map(|(entity, _)| entity)
            .collect()
    };
    let castle = all(&simulation, "TestCastle")[0];
    let bay = all(&simulation, "TestBay")[0];
    let cabs = all(&simulation, "TestCab");
    let fires = |simulation: &Simulation<RotwkRules>, who| {
        simulation
            .rules()
            .lets_passengers_fire(simulation.state(), who)
    };
    assert!(fires(&simulation, cabs[0]), "an open cab with CAN_ATTACK");
    assert!(!fires(&simulation, bay), "an open bay without it");
    order_in(&mut simulation, cabs[0], castle);
    order_in(&mut simulation, cabs[1], bay);
    run(&mut simulation, 300);
    assert!(simulation.state().contains(ids.contained, cabs[0]));
    assert!(simulation.state().contains(ids.contained, cabs[1]));
    assert!(fires(&simulation, cabs[0]), "a cab in a castle");
    assert!(!fires(&simulation, cabs[1]), "a cab in a bay");
}
