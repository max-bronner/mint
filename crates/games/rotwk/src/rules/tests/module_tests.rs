use super::*;
use mint_sage_combat::{ArmorId, WeaponId};
use mint_sage_modules::UpgradeState;

const DATA: &str = "\
Upgrade Upgrade_FireArrows\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_ForgedBlades\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\n  \
  SubUpgradeTemplateNames = Upgrade_HeavyArmor\nEnd\n\
Upgrade Upgrade_HeavyArmor\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Level2\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Level3\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_StructureLevel1\n  Type = OBJECT\nEnd\n\
ExperienceLevel BarracksLevel1\n  TargetNames = Barracks\n  RequiredExperience = 1\n  Rank = 1\n  \
  Upgrades = Upgrade_StructureLevel1\nEnd\n\
Upgrade Upgrade_PlayerArmor\n  Type = PLAYER\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Object Forge\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = ForgeSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
CommandSet ForgeSet\n  1 = Buy_PlayerArmor\nEnd\n\
CommandButton Buy_PlayerArmor\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_PlayerArmor\nEnd\n\
Object Bastion\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = StructureCollapseUpdate ModuleTag_Collapse\n    MinCollapseDelay = 2000\n    \
    MaxCollapseDelay = 2000\n    CollapseDamping = .5\n    DestroyObjectWhenDone = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
Armor ArcherArmor\n  Armor = DEFAULT 100%\nEnd\n\
Armor SoldierArmor\n  Armor = DEFAULT 100%\n  Armor = SLASH 100%\nEnd\n\
Armor SoldierHeavyArmor\n  Armor = DEFAULT 40%\n  Armor = SLASH 50%\nEnd\n\
Armor TargetArmor\n  Armor = DEFAULT 100%\n  Armor = FLAME 200%\nEnd\n\
Weapon ArcherBow\n  AttackRange = 300\n  DelayBetweenShots = 1000\n  WeaponSpeed = 600\n  \
  ProjectileNugget\n    ProjectileTemplateName = Arrow\n    WarheadTemplateName = ArrowHead\n    \
    ForbiddenUpgradeNames = Upgrade_FireArrows\n  End\n  \
  ProjectileNugget\n    ProjectileTemplateName = FireArrow\n    WarheadTemplateName = FireArrowHead\n    \
    RequiredUpgradeNames = Upgrade_FireArrows\n  End\nEnd\n\
Weapon ArrowHead\n  HitStoredTarget = Yes\n  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon FireArrowHead\n  HitStoredTarget = Yes\n  DamageNugget\n    Damage = 10\n    DamageType = FLAME\n  End\nEnd\n\
Weapon Sword\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Weapon SwordUpgraded\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 15\n    DamageType = SLASH\n  End\nEnd\n\
Weapon Claw\n  AttackRange = 60\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ArcherBow\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  WeaponSet\n    Conditions = CLOSE_RANGE CONTESTING_BUILDING\n    Weapon = PRIMARY ArcherBow\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ArcherArmor\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_FireArrowSwap\n    TriggeredBy = Upgrade_FireArrows\n    \
    ShowSubObjects = FireArowTip\n  End\n  \
  Behavior = WeaponSetUpgrade Tag_FireArrowLegality\n    TriggeredBy = Upgrade_FireArrows\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ArcherHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = ArcherHordeSet\n  \
  VisionRange = 400\n  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = StatusBitsUpgrade ModuleTag_ProductionLegality\n    TriggeredBy = Upgrade_FireArrows\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Archer Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10\n  \
  End\nEnd\n\
CommandSet ArcherHordeSet\n  1 = Buy_FireArrows\nEnd\n\
CommandButton Buy_FireArrows\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_FireArrows\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = SoldierHordeSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  WeaponSet\n    Conditions = PLAYER_UPGRADE\n    Weapon = PRIMARY SwordUpgraded\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = SoldierArmor\n  End\n  \
  ArmorSet\n    Conditions = PLAYER_UPGRADE\n    Armor = SoldierHeavyArmor\n  End\n  \
  Behavior = WeaponSetUpgrade Tag_Blades\n    TriggeredBy = Upgrade_ForgedBlades\n  End\n  \
  Behavior = ArmorUpgrade Tag_Armor\n    TriggeredBy = Upgrade_HeavyArmor Upgrade_PlayerArmor\n    \
    ArmorSetFlag = PLAYER_UPGRADE\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_ArmorLook\n    TriggeredBy = Upgrade_HeavyArmor\n    \
    UpgradeTexture = GUManAtArms.tga 0 GUManAtArms_HA.tga\n    ExcludeSubobjects = Forged_Blade\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_BladeLook\n    TriggeredBy = Upgrade_ForgedBlades\n    \
    ShowSubObjects = Forged_Blade\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = SoldierHordeSet\n  \
  VisionRange = 400\n  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = StatusBitsUpgrade ModuleTag_ProductionLegality\n    TriggeredBy = Upgrade_ForgedBlades\n  End\n  \
  Behavior = StatusBitsUpgrade ModuleTag_ProductionLegality2\n    TriggeredBy = Upgrade_HeavyArmor\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    RanksToReleaseWhenAttacking = 1\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10\n  \
  End\nEnd\n\
CommandSet SoldierHordeSet\n  1 = Buy_Blades\n  2 = Buy_Armor\nEnd\n\
CommandButton Buy_Blades\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_ForgedBlades\nEnd\n\
CommandButton Buy_Armor\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_HeavyArmor\nEnd\n\
Object Striker\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Target\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = TargetArmor\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_HideAll\n    TriggeredBy = Upgrade_StructureLevel1\n    \
    HideSubObjects = V1 V2\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_ShowWalls\n    TriggeredBy = Upgrade_Level2\n    \
    ShowSubObjects = V1\n    HideSubObjects = V2\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_ShowTowers\n    TriggeredBy = Upgrade_Level3\n    \
    ShowSubObjects = V1 V2\n  End\n  \
  Behavior = CommandSetUpgrade Tag_Level2Set\n    TriggeredBy = Upgrade_Level2\n    \
    ConflictsWith = Upgrade_Level3\n    CommandSet = BarracksSetLevel2\n  End\n  \
  Behavior = CommandSetUpgrade Tag_Level3Set\n    TriggeredBy = Upgrade_Level3\n    \
    CommandSet = BarracksSetLevel3\n  End\n  \
  Behavior = LevelUpUpgrade Tag_Level2\n    TriggeredBy = Upgrade_Level2\n    \
    LevelsToGain = 1\n    LevelCap = 3\n  End\n  \
  Behavior = RemoveUpgradeUpgrade Tag_DropLevel2\n    TriggeredBy = Upgrade_Level3\n    \
    UpgradeToRemove = Upgrade_Level2\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_ArcherHorde\n  2 = Build_SoldierHorde\n  3 = Buy_Level2\nEnd\n\
CommandSet BarracksSetLevel2\n  1 = Build_ArcherHorde\n  2 = Build_SoldierHorde\n  3 = Build_Striker\n  \
  4 = Buy_Level3\nEnd\n\
CommandSet BarracksSetLevel3\n  1 = Build_ArcherHorde\n  2 = Build_SoldierHorde\n  3 = Build_Striker\n  \
  4 = Build_Target\nEnd\n\
CommandButton Build_ArcherHorde\n  Command = UNIT_BUILD\n  Object = ArcherHorde\n  Radial = Yes\nEnd\n\
CommandButton Build_SoldierHorde\n  Command = UNIT_BUILD\n  Object = SoldierHorde\n  Radial = Yes\nEnd\n\
CommandButton Build_Striker\n  Command = UNIT_BUILD\n  Object = Striker\n  Radial = Yes\nEnd\n\
CommandButton Build_Target\n  Command = UNIT_BUILD\n  Object = Target\n  Radial = Yes\nEnd\n\
CommandButton Buy_Level2\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Level2\n  Radial = Yes\nEnd\n\
CommandButton Buy_Level3\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Level3\n  Radial = Yes\nEnd\n";

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
    let seeds: Vec<String> = ["Barracks", "Striker", "Target", "Forge", "Bastion"]
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

fn rules(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> RotwkRules {
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
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn spawn_horde(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    horde: &str,
    x: f32,
    y: f32,
    owner: PlayerId,
) -> (Entity, Vec<Entity>) {
    let catalogue = simulation.rules().buildable_catalogue();
    let recipe = catalogue
        .iter()
        .find(|entry| entry.label == horde)
        .unwrap_or_else(|| {
            panic!(
                "{horde} is not buildable: {:?}",
                catalogue.iter().map(|e| &e.label).collect::<Vec<_>>()
            )
        })
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

fn state_of(simulation: &Simulation<RotwkRules>, entity: Entity) -> Option<UpgradeState> {
    let id = simulation.rules().upgrade_state_id()?;
    simulation.state().get(id, entity).cloned()
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

fn weapon(m: &Match, name: &str) -> WeaponId {
    m.data.combat.weapons.id(name).unwrap()
}

fn armour(m: &Match, name: &str) -> ArmorId {
    m.data.combat.armors.id(name).unwrap()
}

fn launched(simulation: &mut Simulation<RotwkRules>, ids: Ids, ticks: u32) -> Vec<WeaponId> {
    let flights = |simulation: &Simulation<RotwkRules>| -> Vec<(Entity, WeaponId)> {
        simulation
            .state()
            .store(ids.flight)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(e, flight)| Some((e, flight.warhead?)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut known: Vec<Entity> = flights(simulation).into_iter().map(|(e, _)| e).collect();
    let mut out = Vec::new();
    for _ in 0..ticks {
        run(simulation, 1);
        for (entity, warhead) in flights(simulation) {
            if !known.contains(&entity) {
                known.push(entity);
                out.push(warhead);
            }
        }
    }
    out
}

#[test]
fn fire_arrows_swap_the_warhead_raise_the_flag_and_show_the_tip() {
    let m = load();
    let mut simulation = Simulation::new(
        3,
        rules(
            &m,
            &[
                ("Barracks", 0.0, 0.0, PLAYER),
                ("Target", 360.0, 200.0, OTHER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, archers) = spawn_horde(&mut simulation, ids, "ArcherHorde", 200.0, 200.0, PLAYER);
    assert_eq!(archers.len(), 4);
    for archer in &archers {
        let state = state_of(&simulation, *archer).unwrap_or_default();
        assert_eq!(state.sub_object("FireArowTip"), None);
        assert!(state.weapon_set_flags.is_empty());
    }

    let (plain, fire) = (weapon(&m, "ArrowHead"), weapon(&m, "FireArrowHead"));
    let seen = launched(&mut simulation, ids, 60);
    assert!(
        seen.contains(&plain),
        "the fixture's premise: the archers shoot"
    );
    assert!(!seen.contains(&fire), "no fire arrow before the upgrade");

    research(&mut simulation, archers[0], 0);
    let bit = m.data.upgrade_bits.bit("Upgrade_FireArrows").unwrap();
    let held = |entity| {
        simulation
            .state()
            .get(ids.object_upgrades, entity)
            .is_some_and(|mask| mask.0.contains(bit))
    };
    assert!(held(horde), "the horde researched it");
    assert!(
        archers.iter().all(|a| held(*a)),
        "and every member holds it"
    );
    for archer in &archers {
        let state = state_of(&simulation, *archer).unwrap();
        assert_eq!(state.weapon_set_flags.len(), 1);
        assert!(state.has_weapon_set_flag("PLAYER_UPGRADE"));
        assert!(state
            .model_condition_names()
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("WEAPONSET_PLAYER_UPGRADE")));
        assert_eq!(state.sub_object("FireArowTip"), Some(true));
        let armed = simulation.state().get(ids.armed, *archer).unwrap();
        assert_eq!(
            armed.weapon,
            weapon(&m, "ArcherBow"),
            "no set keys on the flag"
        );
    }

    let seen = launched(&mut simulation, ids, 60);
    assert!(seen.contains(&fire), "fire arrows after the upgrade");
    assert!(
        !seen.contains(&plain),
        "and never the plain arrow they replace"
    );

    let target = simulation
        .state()
        .store(ids.health)
        .unwrap()
        .iter()
        .find(|(entity, _)| {
            simulation
                .state()
                .get(ids.owner, *entity)
                .is_some_and(|owner| owner.0 == OTHER)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let before = health_of(&simulation, ids, target).unwrap();
    let mut drops = Vec::new();
    let mut last = before;
    for _ in 0..60 {
        run(&mut simulation, 1);
        let now = health_of(&simulation, ids, target).unwrap();
        if now < last {
            drops.push(last - now);
        }
        last = now;
    }
    assert!(!drops.is_empty(), "fire arrows land");
    assert!(
        drops.iter().all(|drop| (*drop % 20.0).abs() < f32::EPSILON),
        "every hit is a multiple of 20 (FLAME at 200%), got {drops:?}"
    );
}

#[test]
fn forged_blades_and_heavy_armour_reach_every_member_of_a_horde() {
    let m = load();
    let mut simulation = Simulation::new(
        4,
        rules(
            &m,
            &[
                ("Barracks", 0.0, 0.0, PLAYER),
                ("Striker", 240.0, 200.0, OTHER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (_, soldiers) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    for soldier in &soldiers {
        assert_eq!(
            simulation.state().get(ids.armed, *soldier).unwrap().weapon,
            weapon(&m, "Sword")
        );
        assert_eq!(
            simulation.state().get(ids.armour, *soldier).map(|a| a.0),
            Some(armour(&m, "SoldierArmor"))
        );
    }

    let victim = |simulation: &Simulation<RotwkRules>| -> Option<(Entity, f32)> {
        soldiers.iter().find_map(|s| {
            health_of(simulation, ids, *s)
                .filter(|h| *h < 1000.0)
                .map(|h| (*s, h))
        })
    };
    let mut first = None;
    for _ in 0..120 {
        run(&mut simulation, 1);
        if let Some(hit) = victim(&simulation) {
            first = Some(hit);
            break;
        }
    }
    let (hit, health) = first.expect("the striker must land a hit");
    assert_eq!(health, 990.0, "SoldierArmor takes SLASH at 100%");

    research(&mut simulation, soldiers[0], 0);
    research(&mut simulation, soldiers[0], 1);
    for soldier in &soldiers {
        assert_eq!(
            simulation.state().get(ids.armed, *soldier).unwrap().weapon,
            weapon(&m, "SwordUpgraded"),
            "forged blades"
        );
        assert_eq!(
            simulation.state().get(ids.armour, *soldier).map(|a| a.0),
            Some(armour(&m, "SoldierHeavyArmor")),
            "heavy armour"
        );
        let state = state_of(&simulation, *soldier).unwrap();
        assert_eq!(state.sub_object("Forged_Blade"), Some(true));
        assert_eq!(state.textures.len(), 1);
        assert_eq!(&*state.textures[0].to, "GUManAtArms_HA.tga");
    }

    let before = health_of(&simulation, ids, hit).unwrap();
    let mut after = before;
    for _ in 0..120 {
        run(&mut simulation, 1);
        after = health_of(&simulation, ids, hit).unwrap();
        if after < before {
            break;
        }
    }
    assert_eq!(before - after, 5.0, "SoldierHeavyArmor takes SLASH at 50%");
}

#[test]
fn a_sub_upgrade_is_granted_free_the_instant_its_parent_completes() {
    let m = load();
    let mut simulation = Simulation::new(9, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (_, soldiers) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    for soldier in &soldiers {
        assert_eq!(
            simulation.state().get(ids.armour, *soldier).map(|a| a.0),
            Some(armour(&m, "SoldierArmor")),
            "the fixture's premise: nobody starts upgraded"
        );
    }

    research(&mut simulation, soldiers[0], 0);

    let bits = &m.data.upgrade_bits;
    let held = &simulation
        .state()
        .get(ids.object_upgrades, soldiers[0])
        .unwrap()
        .0;
    assert!(held.contains(bits.bit("Upgrade_ForgedBlades").unwrap()));
    assert!(
        held.contains(bits.bit("Upgrade_HeavyArmor").unwrap()),
        "the sub-upgrade's own bit must be set too"
    );
    for soldier in &soldiers {
        assert_eq!(
            simulation.state().get(ids.armour, *soldier).map(|a| a.0),
            Some(armour(&m, "SoldierHeavyArmor")),
            "ArmorUpgrade's TriggeredBy names Upgrade_HeavyArmor directly - it \
             must react to the granted bit exactly as it would to a bought one"
        );
    }
}

#[test]
fn research_commands_press_each_horde_once() {
    let m = load();
    let mut simulation = Simulation::new(4, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, soldiers) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    spawn_horde(&mut simulation, ids, "SoldierHorde", 900.0, 900.0, OTHER);
    let commands =
        simulation
            .rules()
            .research_commands(simulation.state(), PLAYER, "Upgrade_ForgedBlades");
    let lowest = soldiers.iter().copied().min_by_key(|s| s.index()).unwrap();
    assert_eq!(
        commands,
        vec![
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![lowest],
            },
            RotwkCommand::ObjectUpgrade { button: 0 },
        ],
        "one press for horde {horde:?}"
    );
}

#[test]
fn a_barracks_level_upgrade_swaps_its_command_set_and_its_look() {
    let m = load();
    let mut simulation = Simulation::new(5, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = *simulation
        .state()
        .store(ids.producer)
        .unwrap()
        .entities()
        .first()
        .unwrap();
    let kind = m.kinds.kind("Barracks").unwrap();
    let menu = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.producer, barracks).unwrap().menu
    };
    let buildables = |simulation: &Simulation<RotwkRules>| -> Vec<String> {
        let production = m.kinds.production(&m.data);
        production.menus[usize::from(menu(simulation).unwrap_or(0))]
            .iter()
            .map(|&b| production.buildables[usize::from(b)].label.clone())
            .collect()
    };
    let look = |simulation: &Simulation<RotwkRules>| {
        let state = state_of(simulation, barracks).unwrap();
        (state.sub_object("V1"), state.sub_object("V2"))
    };
    assert_eq!(look(&simulation), (Some(false), Some(false)), "level 1");
    assert_eq!(buildables(&simulation), ["ArcherHorde", "SoldierHorde"]);

    research(&mut simulation, barracks, 0);
    assert_eq!(look(&simulation), (Some(true), Some(false)), "walls");
    assert_eq!(
        buildables(&simulation),
        ["ArcherHorde", "SoldierHorde", "Striker"]
    );
    let state = state_of(&simulation, barracks).unwrap();
    assert_eq!(state.command_set.as_deref(), Some("BarracksSetLevel2"));
    assert_eq!(state.levels_gained, 1);

    let released: Vec<String> = Vec::new();
    let cost = |k: crate::unit::Kind| {
        m.kinds
            .get(k)
            .and_then(|e| e.record.build_cost)
            .and_then(crate::templates::whole)
    };
    let ctx = crate::command_bar::SlotContext {
        passengers: &[],
        passenger_entities: &[],
        capacity: None,
        afford: &|_| true,
        allowed_to_build: &|_| true,
        cost: &cost,
        upgrades: &[],
        object_upgrades: &[],
        player_upgrades: &[],
        castle_upgrades: &[],
        researching: &[],
        upgrade_afford: &|_| true,
        upgrade_affected: &|_| true,
        heroes: &[],
        released_powers: &released,
        command_set: simulation
            .rules()
            .command_set_override(simulation.state(), barracks),
        sell_refusal: None,
        self_repair: None,
        gate: None,
        cancel_refusal: None,
        under_construction: false,
        power_clock: &|_| None,
        auto_cast_on: &|_| false,
        weapon_set_raised: &|_| false,
        model_condition_gate: &|_, _| true,
        foreign_owner: false,
    };
    let bar = crate::command_bar::resolve_command_bar(
        &m.kinds,
        &m.data.command_sets,
        &m.data.command_buttons,
        Some(kind),
        &ctx,
    );
    let shown: Vec<&str> = bar
        .ring
        .iter()
        .flatten()
        .chain(bar.bar.iter())
        .chain(bar.radial.iter().map(|(_, slot)| slot))
        .map(|slot| slot.command_button.as_str())
        .collect();
    assert!(
        shown.contains(&"Buy_Level3") && shown.contains(&"Build_Striker"),
        "the HUD shows the level-2 roster's own buttons: {shown:?}"
    );
    assert!(!shown.contains(&"Buy_Level2"), "{shown:?}");

    research(&mut simulation, barracks, 0);
    assert_eq!(look(&simulation), (Some(true), Some(true)), "towers");
    assert_eq!(
        buildables(&simulation),
        ["ArcherHorde", "SoldierHorde", "Striker", "Target"]
    );
    assert_eq!(
        state_of(&simulation, barracks)
            .unwrap()
            .command_set
            .as_deref(),
        Some("BarracksSetLevel3")
    );
    let bits = &m.data.upgrade_bits;
    let held = &simulation
        .state()
        .get(ids.object_upgrades, barracks)
        .unwrap()
        .0;
    assert!(held.contains(bits.bit("Upgrade_Level3").unwrap()));
    assert!(!held.contains(bits.bit("Upgrade_Level2").unwrap()));
}

#[test]
fn a_match_with_upgrades_completing_mid_fight_replays_frame_for_frame() {
    let m = load();
    let placed = [
        ("Barracks", 0.0, 0.0, PLAYER),
        ("Striker", 240.0, 200.0, OTHER),
    ];
    let mut simulation = Simulation::new(6, rules(&m, &placed)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (_, soldiers) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    let (_, archers) = spawn_horde(&mut simulation, ids, "ArcherHorde", 150.0, 150.0, PLAYER);
    run(&mut simulation, 40);
    research(&mut simulation, soldiers[0], 0);
    research(&mut simulation, archers[0], 0);
    research(&mut simulation, soldiers[1], 1);
    run(&mut simulation, 60);
    let final_hash = simulation.state_hash();
    let upgraded = state_of(&simulation, soldiers[0]).unwrap();
    assert!(
        upgraded.has_weapon_set_flag("PLAYER_UPGRADE") && !upgraded.armor_set_flags.is_empty(),
        "the recording must contain the upgrades it claims to test"
    );

    let recording = simulation.into_recording();
    let verified = recording.verify(rules(&m, &placed)).unwrap();
    assert_eq!(verified.final_hash, final_hash);
}

#[test]
fn object_upgrades_stay_with_their_buyer_and_player_upgrades_reach_new_units() {
    let m = load();
    let mut simulation = Simulation::new(
        8,
        rules(
            &m,
            &[
                ("Barracks", 0.0, 0.0, PLAYER),
                ("Forge", 0.0, 300.0, PLAYER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (_, bought) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    research(&mut simulation, bought[0], 1);
    let (_, fresh) = spawn_horde(&mut simulation, ids, "SoldierHorde", 300.0, 200.0, PLAYER);
    let armour_of = |simulation: &Simulation<RotwkRules>, unit: Entity| {
        simulation.state().get(ids.armour, unit).map(|a| a.0)
    };
    assert_eq!(
        armour_of(&simulation, bought[0]),
        Some(armour(&m, "SoldierHeavyArmor"))
    );
    assert_eq!(
        armour_of(&simulation, fresh[0]),
        Some(armour(&m, "SoldierArmor"))
    );

    let forge = simulation
        .state()
        .store(ids.producer)
        .unwrap()
        .iter()
        .find(|(_, producer)| producer.player_menu.is_some())
        .map(|(entity, _)| entity)
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![forge],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    run(&mut simulation, 35);
    assert_eq!(
        armour_of(&simulation, fresh[0]),
        Some(armour(&m, "SoldierHeavyArmor")),
        "a player upgrade reaches every existing unit"
    );
    let (_, later) = spawn_horde(&mut simulation, ids, "SoldierHorde", 400.0, 200.0, PLAYER);
    assert_eq!(
        armour_of(&simulation, later[0]),
        Some(armour(&m, "SoldierHeavyArmor")),
        "and every unit created after it"
    );
}

#[test]
fn a_selected_members_buttons_are_its_hordes() {
    let m = load();
    let mut simulation =
        Simulation::new(4, rules(&m, &[("Striker", 600.0, 600.0, OTHER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, "SoldierHorde", 200.0, 200.0, PLAYER);
    let world = simulation.state();
    let rules = simulation.rules();
    assert_eq!(rules.command_subject(world, members[0]), horde);
    assert_eq!(
        world.get(ids.kind, horde).copied(),
        m.kinds.kind("SoldierHorde"),
        "the subject carries the horde's own template"
    );
    let striker = world
        .store(ids.kind)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|e| world.get(ids.kind, *e).copied() == m.kinds.kind("Striker"))
        .unwrap();
    assert_eq!(rules.command_subject(world, striker), striker);
}

#[test]
fn a_structures_own_collapse_decides_when_it_goes_and_replays_frame_for_frame() {
    let m = load();
    let placed = [("Bastion", 0.0, 0.0, PLAYER)];
    let mut simulation = Simulation::new(12, rules(&m, &placed)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let bastion = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|e| simulation.state().get(ids.kind, *e).copied() == m.kinds.kind("Bastion"))
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![bastion],
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    let killed_at = simulation.frame();
    run(&mut simulation, 1);
    let doomed = simulation
        .state()
        .get(ids.doomed, bastion)
        .copied()
        .expect("DebugKill resolved a death, which always marks the corpse doomed");
    assert_eq!(doomed.destroy_at, Frame::FOREVER, "no placeholder timer");
    assert_eq!(killed_at, Frame::new(1));

    run(&mut simulation, 68);
    assert!(
        simulation.state().is_live(bastion),
        "not yet - still waiting or sinking"
    );
    run(&mut simulation, 1);
    assert!(
        !simulation.state().is_live(bastion),
        "removed the frame it has sunk its full height"
    );

    let final_hash = simulation.state_hash();
    let recording = simulation.into_recording();
    let verified = recording.verify(rules(&m, &placed)).unwrap();
    assert_eq!(verified.final_hash, final_hash);
}
