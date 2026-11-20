use super::*;
use crate::unit::Kind;

const DATA: &str = "\
Locomotor HeroFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
SpecialPower SpecialAbilityRage\n  ReloadTime = 30000\nEnd\n\
ModifierList Rage\n  Category = SPELL\n  Modifier = DAMAGE_MULT 150%\n  Duration = 3000\nEnd\n\
CommandSet AutoSet\n  1 = Fire_Rage\nEnd\n\
CommandButton Fire_Rage\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\nEnd\n\
CommandSet AutoRangedSet\n  1 = Fire_Rage_Ranged\nEnd\n\
CommandButton Fire_Rage_Ranged\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\n  PresetRange = 200\nEnd\n\
Object AutoCasterOff\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterOn\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterTwoModules\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = DualAutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_AutoA\n    SpecialAbility = SpecialAbilityRage\n    Query = 1 ALL\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_AutoB\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterPair\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 2 ALL\n    MaxScanRange = 200\n    \
    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet TriggerSet\n  1 = Fire_Rage_Trigger\nEnd\n\
CommandButton Fire_Rage_Trigger\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\n  TriggerWhenReady = Yes\nEnd\n\
Object AutoCasterTrigger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = TriggerSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterVision\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL ENEMIES\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ForbiddenSet\n  1 = Buy_Boost\n  2 = Fire_Rage\nEnd\n\
Object AutoCasterForbidden\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ForbiddenSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Status\n    TriggeredBy = Upgrade_Boost\n    StatusToSet = DO_NOT_PICK_ME\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL ENEMIES\n    MaxScanRange = 200\n    \
    ForbiddenStatus = DO_NOT_PICK_ME\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterFree\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL ENEMIES\n    MaxScanRange = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DominateAutoSet\n  1 = Cast_Dominate_Auto\nEnd\n\
CommandButton Cast_Dominate_Auto\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  AutoAbility = Yes\n  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
SpecialPower SpecialAbilityDominate\n  ReloadTime = 0\nEnd\n\
Object AutoDominator\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = DominateAutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Stick\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 200\n    AttributeModifierAffects = ALL ENEMIES\n    \
    PermanentlyConvert = Yes\n    IgnoreFacingCheck = Yes\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object AutoCasterRanged\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoRangedSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL ENEMIES\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Nearby\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
SpecialPower SpecialAbilityFrenzy\n  ReloadTime = 30000\nEnd\n\
CommandSet DualAutoSet\n  1 = Fire_Rage_Dual\n  2 = Fire_Frenzy_Dual\nEnd\n\
CommandButton Fire_Rage_Dual\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\nEnd\n\
CommandButton Fire_Frenzy_Dual\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityFrenzy\n  \
  AutoAbility = Yes\nEnd\n\
Object AutoCasterDual\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = DualAutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet AutoPosSet\n  1 = Fire_Rage_Pos\nEnd\n\
CommandButton Fire_Rage_Pos\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\n  Options = NEED_TARGET_POS\nEnd\n\
Object AutoCasterPos\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoPosSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Hero\nEnd\n\
CommandButton Fire_Hero\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\nEnd\n\
Weapon Stick\n  AttackRange = 50\n  DelayBetweenShots = 100000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object AutoCasterHero\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Stick\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = HeroFeet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Behavior = HeroModeSpecialAbilityUpdate Tag_Hero\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    UnpackTime = 200\n    PreparationTime = 200\n    HeroAttributeModifier = Rage\n    \
    HeroEffectDuration = 3000\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object DistantVictim\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SecondVictim\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DamagedSet\n  1 = Fire_Rage_Damaged\nEnd\n\
CommandButton Fire_Rage_Damaged\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\n  NeedDamagedTarget = Yes\nEnd\n\
Object AutoCasterDamaged\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = DamagedSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    MaxScanRange = 200\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object DamageableTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Upgrade Upgrade_Boost\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
CommandSet GatedSet\n  1 = Buy_Boost\n  2 = Fire_Rage_Gated\nEnd\n\
CommandButton Buy_Boost\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Boost\nEnd\n\
CommandButton Fire_Rage_Gated\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityRage\n  \
  AutoAbility = Yes\n  EnableOnModelCondition = BOOSTED\nEnd\n\
Object AutoCasterGated\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = GatedSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    StartsActive = Yes\n  End\n  \
  Behavior = ModelConditionUpgrade Tag_Boost\n    TriggeredBy = Upgrade_Boost\n    \
    AddConditionFlags = BOOSTED\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object LockArmed\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Stick\n    Weapon = SECONDARY Stick\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n    LockWeaponSlot = SECONDARY\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object LockUnarmed\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = AutoSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Stick\n  End\n  \
  Behavior = WeaponModeSpecialPowerUpdate Tag_Rage\n    SpecialPowerTemplate = SpecialAbilityRage\n    \
    Duration = 3000\n    AttributeModifier = Rage\n    LockWeaponSlot = SECONDARY\n  End\n  \
  Behavior = AutoAbilityBehavior Tag_Auto\n    Query = 1 ALL\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

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
        "AutoCasterOff",
        "AutoCasterOn",
        "AutoCasterPair",
        "AutoCasterTwoModules",
        "AutoCasterTrigger",
        "AutoCasterVision",
        "AutoCasterForbidden",
        "AutoCasterFree",
        "AutoDominator",
        "AutoCasterRanged",
        "AutoCasterDual",
        "AutoCasterPos",
        "AutoCasterHero",
        "DistantVictim",
        "SecondVictim",
        "AutoCasterDamaged",
        "DamageableTarget",
        "AutoCasterGated",
        "LockArmed",
        "LockUnarmed",
        "Nearby",
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

fn simulation(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
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

fn damage_mult(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(1.0, |bonuses| bonuses.0.damage_mult)
}

fn only_of(simulation: &Simulation<RotwkRules>, ids: Ids, kind: Kind) -> Entity {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found[0]
}

fn all_of(simulation: &Simulation<RotwkRules>, ids: Ids, kind: Kind) -> Vec<Entity> {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found
}

fn on(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .rules()
        .autocast_on(simulation.state(), ids, entity, "SpecialAbilityRage")
}

fn select(simulation: &mut Simulation<RotwkRules>, units: Vec<Entity>) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
}

#[test]
fn auto_cast_is_off_until_enabled() {
    let m = load();
    let off = m.kinds.kind("AutoCasterOff").unwrap();
    let on = m.kinds.kind("AutoCasterOn").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterOff", 100.0, 100.0, PLAYER),
            ("AutoCasterOn", 100.0, 200.0, PLAYER),
            ("Nearby", 150.0, 100.0, PLAYER),
            ("Nearby", 150.0, 200.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let off_entity = only_of(&simulation, ids, off);
    let on_entity = only_of(&simulation, ids, on);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        damage_mult(&simulation, ids, off_entity),
        1.0,
        "StartsActive defaults to No - it must not have cast"
    );
    assert!(
        (damage_mult(&simulation, ids, on_entity) - 1.5).abs() < 1e-6,
        "StartsActive = Yes casts with no toggle"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![off_entity],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        (damage_mult(&simulation, ids, off_entity) - 1.5).abs() < 1e-6,
        "toggled on, it now casts"
    );
}

#[test]
fn a_bare_block_aims_at_the_closest_enemy_not_the_closest_object() {
    let m = load();
    let kind = m.kinds.kind("AutoDominator").unwrap();
    let nearby = m.kinds.kind("Nearby").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoDominator", 100.0, 100.0, PLAYER),
            ("Nearby", 150.0, 100.0, PLAYER),
            ("Nearby", 200.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let _ = only_of(&simulation, ids, kind);
    let enemy = all_of(&simulation, ids, nearby)[1];
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.owner, enemy)
            .map(|owner| owner.0),
        Some(PLAYER),
        "the enemy, not the nearer ally, was the target"
    );
}

#[test]
fn a_query_count_includes_the_caster() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterPair").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterPair", 100.0, 100.0, PLAYER),
            ("Nearby", 150.0, 100.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!((damage_mult(&simulation, ids, caster) - 1.5).abs() < 1e-6);
}

#[test]
fn an_unranged_block_scans_the_casters_vision_range() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterVision").unwrap();
    let run = |enemy_at: f32| {
        let mut simulation = simulation(
            &m,
            &[
                ("AutoCasterVision", 100.0, 100.0, PLAYER),
                ("Nearby", 100.0 + enemy_at, 100.0, OTHER),
            ],
        );
        let ids = simulation.rules().ids().unwrap();
        let caster = only_of(&simulation, ids, kind);
        for _ in 0..10 {
            simulation.tick().unwrap();
        }
        damage_mult(&simulation, ids, caster)
    };
    assert_eq!(run(150.0), 1.0, "past the vision range of 100");
    assert!((run(80.0) - 1.5).abs() < 1e-6, "inside the vision range");
}

#[test]
fn forbidden_status_is_read_on_the_caster() {
    let m = load();
    let forbidden = m.kinds.kind("AutoCasterForbidden").unwrap();
    let free = m.kinds.kind("AutoCasterFree").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterForbidden", 100.0, 100.0, PLAYER),
            ("AutoCasterFree", 100.0, 200.0, PLAYER),
            ("Nearby", 150.0, 100.0, OTHER),
            ("Nearby", 150.0, 200.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let forbidden = only_of(&simulation, ids, forbidden);
    let free = only_of(&simulation, ids, free);
    select(&mut simulation, vec![forbidden, free]);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!((damage_mult(&simulation, ids, free) - 1.5).abs() < 1e-6);
    assert_eq!(damage_mult(&simulation, ids, forbidden), 1.0);
}

#[test]
fn trigger_when_ready_casts_without_a_target() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterTrigger").unwrap();
    let mut simulation = simulation(&m, &[("AutoCasterTrigger", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!((damage_mult(&simulation, ids, caster) - 1.5).abs() < 1e-6);
}

#[test]
fn an_unranged_block_scans_the_buttons_preset_range() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterRanged").unwrap();
    let mut far = simulation(
        &m,
        &[
            ("AutoCasterRanged", 100.0, 100.0, PLAYER),
            ("Nearby", 2_100.0, 100.0, OTHER),
        ],
    );
    let ids = far.rules().ids().unwrap();
    let caster = only_of(&far, ids, kind);
    for _ in 0..10 {
        far.tick().unwrap();
    }
    assert_eq!(
        damage_mult(&far, ids, caster),
        1.0,
        "2,100 units is past the button's PresetRange of 200 - it must not have cast"
    );
    let mut near = simulation(
        &m,
        &[
            ("AutoCasterRanged", 100.0, 100.0, PLAYER),
            ("Nearby", 250.0, 100.0, OTHER),
        ],
    );
    let ids = near.rules().ids().unwrap();
    let caster = only_of(&near, ids, kind);
    for _ in 0..10 {
        near.tick().unwrap();
    }
    assert!(
        (damage_mult(&near, ids, caster) - 1.5).abs() < 1e-6,
        "150 units inside PresetRange - it must have cast"
    );
}

#[test]
fn enabling_a_second_button_takes_the_slot_from_the_first() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterDual").unwrap();
    let mut simulation = simulation(&m, &[("AutoCasterDual", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let entity = only_of(&simulation, ids, kind);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        on(&simulation, ids, entity),
        "the first button took the slot"
    );
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 1 })
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !on(&simulation, ids, entity),
        "the second button took the slot from the first"
    );
    assert!(simulation.rules().autocast_on(
        simulation.state(),
        ids,
        entity,
        "SpecialAbilityFrenzy"
    ));
}

#[test]
fn toggling_the_holder_off_leaves_the_slot_empty() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterDual").unwrap();
    let mut simulation = simulation(&m, &[("AutoCasterDual", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let entity = only_of(&simulation, ids, kind);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    assert!(on(&simulation, ids, entity));
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !on(&simulation, ids, entity),
        "the holder's own button turns the slot off"
    );
}

#[test]
fn the_auto_cast_slot_hashes_identically_across_two_runs_of_the_same_commands() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterDual").unwrap();
    let run = || {
        let mut simulation = simulation(&m, &[("AutoCasterDual", 100.0, 100.0, PLAYER)]);
        let ids = simulation.rules().ids().unwrap();
        let entity = only_of(&simulation, ids, kind);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![entity],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
            .unwrap();
        simulation.tick().unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 1 })
            .unwrap();
        simulation.tick().unwrap();
        simulation.state_hash()
    };
    assert_eq!(run(), run(), "the same commands hash identically");
}

#[test]
fn a_need_target_pos_button_auto_casts_at_the_scanned_targets_position() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterPos").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterPos", 100.0, 100.0, PLAYER),
            ("Nearby", 150.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        (damage_mult(&simulation, ids, caster) - 1.5).abs() < 1e-6,
        "a NEED_TARGET_POS auto-cast must fire at the scanned target's position, not be skipped"
    );
}

#[test]
fn an_auto_cast_mid_fight_resumes_the_attack_order_afterwards() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterHero").unwrap();
    let victim_kind = m.kinds.kind("DistantVictim").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterHero", 100.0, 100.0, PLAYER),
            ("DistantVictim", 100.0, 450.0, OTHER),
            ("Nearby", 150.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    let victim = only_of(&simulation, ids, victim_kind);
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
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.attack_order, caster)
            .map(|o| o.target),
        Some(victim),
        "the attack order must be in place before the cast starts"
    );
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    let mut saw_cleared = false;
    let mut resumed = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.attack_order, caster) {
            saw_cleared = true;
        } else if saw_cleared
            && simulation
                .state()
                .get(ids.attack_order, caster)
                .is_some_and(|order| order.target == victim)
        {
            resumed = true;
            break;
        }
    }
    assert!(
        saw_cleared,
        "the cast must have dropped the attack order at some point"
    );
    assert!(
        resumed,
        "the attack order on victim must resume once the cast exits"
    );
}

#[test]
fn a_player_order_mid_cast_survives_the_casts_own_exit() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterHero").unwrap();
    let victim_kind = m.kinds.kind("DistantVictim").unwrap();
    let second_kind = m.kinds.kind("SecondVictim").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterHero", 100.0, 100.0, PLAYER),
            ("DistantVictim", 100.0, 450.0, OTHER),
            ("SecondVictim", 400.0, 100.0, OTHER),
            ("Nearby", 150.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    let victim = only_of(&simulation, ids, victim_kind);
    let second_victim = only_of(&simulation, ids, second_kind);
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
        .submit(PLAYER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    let mut gave_new_order = false;
    let mut exited = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if !gave_new_order && simulation.state().contains(ids.no_auto_acquire, caster) {
            simulation
                .submit(
                    PLAYER,
                    RotwkCommand::AttackObject {
                        target: second_victim,
                    },
                )
                .unwrap();
            gave_new_order = true;
        }
        if gave_new_order && !simulation.state().contains(ids.no_auto_acquire, caster) {
            exited = true;
            break;
        }
    }
    assert!(
        gave_new_order,
        "the fixture's premise: the cast must run long enough to order into it"
    );
    assert!(
        exited,
        "the cast (aborted by the order) must have exited by the budget's end"
    );
    assert_eq!(
        simulation
            .state()
            .get(ids.attack_order, caster)
            .map(|o| o.target),
        Some(second_victim),
        "the order given mid-cast must survive the cast's exit, not be overwritten by the stash"
    );
}

#[test]
fn need_damaged_target_refuses_an_undamaged_candidate_then_fires_once_hurt() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterDamaged").unwrap();
    let target_kind = m.kinds.kind("DamageableTarget").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterDamaged", 100.0, 100.0, PLAYER),
            ("DamageableTarget", 150.0, 100.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    let target = only_of(&simulation, ids, target_kind);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        damage_mult(&simulation, ids, caster),
        1.0,
        "an undamaged candidate must refuse NeedDamagedTarget - the cast must not fire"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![target],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Damage { percent: 25 }),
        )
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        (damage_mult(&simulation, ids, caster) - 1.5).abs() < 1e-6,
        "a candidate at 75% health passes NeedDamagedTarget - the cast must fire"
    );
}

#[test]
fn a_failed_button_gate_switches_the_module_off_until_toggled_again() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterGated").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("AutoCasterGated", 100.0, 100.0, PLAYER),
            ("Nearby", 150.0, 100.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(damage_mult(&simulation, ids, caster), 1.0, "gate closed");
    assert!(
        !on(&simulation, ids, caster),
        "the failed gate cleared the switch"
    );
    select(&mut simulation, vec![caster]);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..80 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        damage_mult(&simulation, ids, caster),
        1.0,
        "BOOSTED is held now, but the switch stays off"
    );
    simulation
        .submit(PLAYER, RotwkCommand::ToggleAutoCast { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!((damage_mult(&simulation, ids, caster) - 1.5).abs() < 1e-6);
}

#[test]
fn each_module_keeps_its_own_active_button() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterTwoModules").unwrap();
    let mut simulation = simulation(&m, &[("AutoCasterTwoModules", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let entity = only_of(&simulation, ids, kind);
    select(&mut simulation, vec![entity]);
    for button in [0, 1] {
        simulation
            .submit(PLAYER, RotwkCommand::ToggleAutoCast { button })
            .unwrap();
        simulation.tick().unwrap();
    }
    let both = ["SpecialAbilityRage", "SpecialAbilityFrenzy"]
        .iter()
        .all(|power| {
            simulation
                .rules()
                .autocast_on(simulation.state(), ids, entity, power)
        });
    assert!(
        both,
        "the configured module serves one power, the bare module the other"
    );
}

#[test]
fn one_held_flag_of_several_opens_enable_and_closes_disable() {
    let m = load();
    let kind = m.kinds.kind("AutoCasterGated").unwrap();
    let mut simulation = simulation(&m, &[("AutoCasterGated", 100.0, 100.0, PLAYER)]);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_of(&simulation, ids, kind);
    let flags = ["USER_1".to_owned(), "BOOSTED".to_owned()];
    let gate = |simulation: &Simulation<RotwkRules>, enable: &[String], disable: &[String]| {
        simulation
            .rules()
            .model_condition_gate(simulation.state(), caster, enable, disable)
    };
    simulation.tick().unwrap();
    assert!(!gate(&simulation, &flags, &[]), "nothing held yet");
    assert!(gate(&simulation, &[], &flags));
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
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(gate(&simulation, &flags, &[]), "BOOSTED alone opens it");
    assert!(!gate(&simulation, &[], &flags), "BOOSTED alone closes it");
}

#[test]
fn a_weapon_lock_closes_its_condition_gate_only_on_an_armed_slot() {
    let m = load();
    let armed = m.kinds.kind("LockArmed").unwrap();
    let unarmed = m.kinds.kind("LockUnarmed").unwrap();
    let mut simulation = simulation(
        &m,
        &[
            ("LockArmed", 100.0, 100.0, PLAYER),
            ("LockUnarmed", 100.0, 200.0, PLAYER),
            ("Nearby", 150.0, 100.0, PLAYER),
            ("Nearby", 150.0, 200.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let armed = only_of(&simulation, ids, armed);
    let unarmed = only_of(&simulation, ids, unarmed);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let lock = ["WEAPONLOCK_SECONDARY".to_owned()];
    let open = |entity: Entity| {
        simulation
            .rules()
            .model_condition_gate(simulation.state(), entity, &[], &lock)
    };
    assert!((damage_mult(&simulation, ids, armed) - 1.5).abs() < 1e-6);
    assert!(!open(armed), "the locked slot's condition closes the gate");
    assert!((damage_mult(&simulation, ids, unarmed) - 1.5).abs() < 1e-6);
    assert!(open(unarmed), "an empty slot refuses the lock");
}
