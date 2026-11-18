use super::*;

const DATA: &str = "\
GameData\n  SellPercentage = 50%\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Claw\n  AttackRange = 200\n  DelayBetweenShots = 300\n  \
  DamageNugget\n    Damage = 400\n    DamageType = SLASH\n  End\nEnd\n\
ModifierList Outlaw\n  Category = LEADERSHIP\n  Modifier = BOUNTY_PERCENTAGE 100.0%\n  \
  Duration = 0\nEnd\n\
Object Raider\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  VisionRange = 400\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = AttributeModifierAuraUpdate Tag_Outlaw\n    StartsActive = Yes\n    \
    BonusName = Outlaw\n    Range = 400\n    AllowSelf = Yes\n    \
    ObjectFilter = ANY +INFANTRY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Lair\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = LairSet\n  BuildCost = 200\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1700\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    RebuildWhenDead = Yes\n    \
    UseSpawnTimerWithoutWorker = Yes\n    RebuildTimeSeconds = 2\n    \
    DisallowRebuildRange = 260\n    DisallowRebuildFilter = ANY +STRUCTURE\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    CollapseDamping = .5\n    \
    DestroyObjectWhenDone = Yes\n  End\n  \
  Behavior = RebuildHoleExposeDie Tag_Expose\n    ExemptStatus = SOLD\n    \
    HoleName = LairHole\n    HoleMaxHealth = 430.0\n    FadeInTimeSeconds = 1.5\n    \
    TransferAttackers = No\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object LairHole\n  Side = Wild\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE REBUILD_HOLE\n  BuildCost = 90\n  BuildTime = 12\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 8888888.8\n    InitialHealth = 8888888.8\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = RebuildHoleBehavior Tag_Hole\n    WorkerRespawnDelay = 2000\n    \
    HoleHealthRegen%PerSecond = 0.4%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 24\n  GeometryHeight = 6\nEnd\n\
Object Watchtower\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 300\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  \
  GeometryHeight = 30\nEnd\n\
Object WorkerlessLair\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = LairSet\n  BuildCost = 200\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1700\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    WorkerName = ZDiggerNoSelect\n    \
    SpawnTimer = 240\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    CollapseDamping = .5\n    \
    DestroyObjectWhenDone = Yes\n  End\n  \
  Behavior = RebuildHoleExposeDie Tag_Expose\n    ExemptStatus = SOLD\n    \
    HoleName = WorkerlessLairHole\n    HoleMaxHealth = 430.0\n    FadeInTimeSeconds = 1.5\n    \
    TransferAttackers = No\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object WorkerlessLairHole\n  Side = Wild\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE REBUILD_HOLE\n  BuildCost = 90\n  BuildTime = 12\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 8888888.8\n    InitialHealth = 8888888.8\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = RebuildHoleBehavior Tag_Hole\n    WorkerRespawnDelay = 2000\n    \
    HoleHealthRegen%PerSecond = 0.4%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 24\n  GeometryHeight = 6\nEnd\n\
Object ZDiggerNoSelect\n  Side = Mordor\n  KindOf = INFANTRY\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 12\nEnd\n\
Locomotor Paws\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
ObjectCreationList OCL_Loot\n  CreateObject\n    ObjectNames = Loot\n    Count = 1\n  End\nEnd\n\
ObjectCreationList OCL_Scatter\n  CreateObject\n    ObjectNames = Loot\n    Count = 5\n    \
    SpreadFormation = Yes\n    MinDistanceAFormation = 30.0\n    MinDistanceBFormation = 36.0\n    \
    MaxDistanceFormation = 70.0\n    IgnoreAllObjects = Yes\n  End\nEnd\n\
Object Hoard\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1700\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Behavior = RebuildHoleExposeDie Tag_Expose\n    HoleName = HoardHole\n    HoleMaxHealth = 50.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 18\n  GeometryHeight = 20\nEnd\n\
Object HoardHole\n  Side = Wild\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE REBUILD_HOLE\n  BuildCost = 90\n  BuildTime = 12\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 8888888.8\n  End\n  \
  Behavior = RebuildHoleBehavior Tag_Hole\n    WorkerRespawnDelay = 100000\n  End\n  \
  Behavior = CreateObjectDie Tag_Loot\n    DeathTypes = ALL -FADED\n    CreationList = OCL_Scatter\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 24\n  GeometryHeight = 6\nEnd\n\
Object Den\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1700\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    RebuildTimeSeconds = 2\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
    SpawnTemplateName = Pup\n    SpawnReplaceDelay = 100000\n    CanReclaimOrphans = Yes\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    CollapseDamping = .5\n    \
    DestroyObjectWhenDone = Yes\n  End\n  \
  Behavior = RebuildHoleExposeDie Tag_Expose\n    HoleName = DenHole\n    HoleMaxHealth = 430.0\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object DenHole\n  Side = Wild\n  \
  KindOf = STRUCTURE SELECTABLE IMMOBILE REBUILD_HOLE\n  BuildCost = 90\n  BuildTime = 12\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 8888888.8\n  End\n  \
  Behavior = RebuildHoleBehavior Tag_Hole\n    WorkerRespawnDelay = 2000\n  End\n  \
  Behavior = CreateObjectDie Tag_Loot\n    DeathTypes = ALL -FADED\n    CreationList = OCL_Loot\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 24\n  GeometryHeight = 6\nEnd\n\
Object Pup\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Paws\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 60\n    GuardWanderRange = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Loot\n  Side = Wild\n  KindOf = IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 5\nEnd\n\
Object Ruin\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  KeepSelectableWhenDead = Yes\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    RebuildTimeSeconds = 2\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object BookendRuin\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  KeepSelectableWhenDead = Yes\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\n  \
  AdditionalGeometry = BOX\n  GeometryName = Bookend\n  GeometryMajorRadius = 3\n  \
  GeometryMinorRadius = 3\n  GeometryHeight = 53\nEnd\n\
Object RuinPriced\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  KeepSelectableWhenDead = Yes\n  \
  BuildCost = 4000\n  BuildTime = 10\n  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    RebuildTimeSeconds = 2\n    \
    PercentOfBuildCostToRebuildRubble = 30%\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object RuinDear\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  KeepSelectableWhenDead = Yes\n  \
  BuildCost = 4500\n  BuildTime = 10\n  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    RebuildTimeSeconds = 2\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
CommandSet LairSet\n  1 = Command_Sell\nEnd\n\
CommandButton Command_Sell\n  Command = SELL\n  InPalantir = Yes\nEnd\n";

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
        "Lair",
        "Raider",
        "Watchtower",
        "WorkerlessLair",
        "Den",
        "Hoard",
        "Ruin",
        "BookendRuin",
        "RuinPriced",
        "RuinDear",
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

fn start(b: &Base, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    let production = b.kinds.production(&b.data);
    let power_names = crate::powers::PowerNames::build(b.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&b.data.special_powers);
    let mut footprints = Vec::new();
    let spawns: Vec<Spawn> = placed
        .iter()
        .enumerate()
        .map(|(at, (name, x, y, owner))| {
            let kind = b.kinds.kind(name).unwrap();
            let spawn = b
                .kinds
                .spawn(
                    &b.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap();
            let record = &b.kinds.get(kind).unwrap().record;
            let records = crate::construction::footprint_of(record);
            if !records.is_empty() {
                footprints.push((at, records));
            }
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_structure_footprints(footprints)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone())
        .with_upgrades(b.kinds.upgrades(&b.data), Arc::clone(&b.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_sell_percentage(b.data.sell_percentage)
        .with_creation_lists(Arc::clone(&b.data.ocls));
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn of_kind(b: &Base, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = b.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

fn press(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    on: Entity,
    command: RotwkCommand,
) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![on],
            },
        )
        .unwrap();
    simulation.submit(player, command).unwrap();
    run(simulation, 1);
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

#[test]
fn a_dead_structure_leaves_a_hole_that_rebuilds_it_after_the_respawn_delay() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Lair", 400.0, 400.0, PLAYER)]);
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let holes = of_kind(&b, &simulation, ids, "LairHole");
    assert_eq!(holes.len(), 1, "one hole where the lair stood");
    let hole = holes[0];
    let health = simulation.state().get(ids.health, hole).unwrap().0;
    assert_eq!(
        health.max(),
        430.0,
        "HoleMaxHealth, not the body's 8888888.8"
    );
    assert_eq!(
        simulation.state().get(ids.body, hole).unwrap().0.position,
        Vec2::new(400.0, 400.0)
    );
    run(&mut simulation, 55);
    assert!(
        of_kind(&b, &simulation, ids, "Lair").is_empty(),
        "still waiting"
    );
    run(&mut simulation, 10);
    let rebuilt = of_kind(&b, &simulation, ids, "Lair");
    assert_eq!(rebuilt.len(), 1, "the lair is back");
    assert!(
        simulation.state().contains(ids.construction, rebuilt[0]),
        "as a site, not finished at once"
    );
    run(&mut simulation, 61);
    let health = simulation.state().get(ids.health, rebuilt[0]).unwrap().0;
    assert_eq!(health.current(), 1700.0, "rebuilt to full health");
    run(&mut simulation, 2);
    assert!(
        gone(&simulation, ids, hole),
        "the hole goes once the lair is finished"
    );
}

fn gone(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    !simulation.state().is_live(entity) || simulation.state().contains(ids.doomed, entity)
}

fn masked(simulation: &Simulation<RotwkRules>, hole: Entity) -> bool {
    let wiring = simulation.rules().modules.as_ref().unwrap();
    wiring.carries_status(simulation.state(), hole, "UNSELECTABLE")
        && wiring.carries_status(simulation.state(), hole, "UNATTACKABLE")
}

#[test]
fn a_hole_is_masked_under_its_rebuild_and_fades_without_its_treasure() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Den", 400.0, 400.0, PLAYER)]);
    let den = of_kind(&b, &simulation, ids, "Den")[0];
    press(&mut simulation, PLAYER, den, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let hole = of_kind(&b, &simulation, ids, "DenHole")[0];
    assert!(!masked(&simulation, hole), "a hole waiting is a target");
    run(&mut simulation, 65);
    let site = of_kind(&b, &simulation, ids, "Den");
    assert_eq!(site.len(), 1, "the den is growing back");
    assert!(simulation.state().is_live(hole), "over its hole");
    assert!(masked(&simulation, hole), "which is masked meanwhile");
    run(&mut simulation, 64);
    assert!(
        !simulation.state().contains(ids.construction, site[0]),
        "finished"
    );
    assert!(
        gone(&simulation, ids, hole),
        "the hole went with the den finished"
    );
    run(&mut simulation, 2);
    assert!(
        of_kind(&b, &simulation, ids, "Loot").is_empty(),
        "a FADED hole drops no treasure"
    );
    press(&mut simulation, PLAYER, site[0], RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let second = of_kind(&b, &simulation, ids, "DenHole")
        .into_iter()
        .find(|other| !gone(&simulation, ids, *other))
        .unwrap();
    press(&mut simulation, PLAYER, second, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    assert_eq!(
        of_kind(&b, &simulation, ids, "Loot").len(),
        1,
        "a hole destroyed outright drops it"
    );
}

#[test]
fn a_site_destroyed_while_it_rebuilds_unmasks_its_hole_which_rebuilds_again() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Lair", 400.0, 400.0, PLAYER)]);
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 66);
    let hole = of_kind(&b, &simulation, ids, "LairHole")[0];
    let site = of_kind(&b, &simulation, ids, "Lair")[0];
    assert!(simulation.state().contains(ids.construction, site));
    press(&mut simulation, PLAYER, site, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    assert_eq!(
        of_kind(&b, &simulation, ids, "LairHole"),
        vec![hole],
        "the same hole, and no second one"
    );
    assert!(!masked(&simulation, hole), "unmasked");
    run(&mut simulation, 65);
    let again = of_kind(&b, &simulation, ids, "Lair");
    assert_eq!(again.len(), 1, "the lair grows back a second time");
    assert!(simulation.state().contains(ids.construction, again[0]));
}

#[test]
fn destroying_the_hole_pays_its_build_cost_as_a_bounty() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Lair", 400.0, 400.0, PLAYER),
            ("Raider", 400.0, 250.0, OTHER),
        ],
    );
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let hole = of_kind(&b, &simulation, ids, "LairHole")[0];
    let before = purse(&simulation, ids, OTHER);
    for _ in 0..50 {
        if simulation.state().contains(ids.doomed, hole) {
            break;
        }
        run(&mut simulation, 1);
    }
    assert!(
        simulation.state().contains(ids.doomed, hole),
        "the raider killed the hole"
    );
    assert_eq!(
        purse(&simulation, ids, OTHER) - before,
        90,
        "the hole's BuildCost"
    );
}

#[test]
fn a_sold_structure_leaves_no_hole() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Lair", 400.0, 400.0, PLAYER)]);
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::Sell);
    assert!(simulation.state().contains(ids.doomed, lair), "sold");
    run(&mut simulation, 2);
    assert!(
        of_kind(&b, &simulation, ids, "LairHole").is_empty(),
        "a sale leaves no hole"
    );
}

#[test]
fn a_structure_in_range_when_it_dies_does_not_stop_its_hole_rebuilding() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Lair", 400.0, 400.0, PLAYER),
            ("Watchtower", 400.0, 550.0, OTHER),
        ],
    );
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 66);
    assert_eq!(
        of_kind(&b, &simulation, ids, "Lair").len(),
        1,
        "the lair grows back beside the tower"
    );
}

#[test]
fn a_rebuilt_spawner_reclaims_its_freed_slave_once_built_and_the_slave_guards_again() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Den", 400.0, 400.0, PLAYER)]);
    run(&mut simulation, 30);
    let pups = of_kind(&b, &simulation, ids, "Pup");
    assert_eq!(pups.len(), 1, "the den made its pup");
    let pup = pups[0];
    let slaved_to = simulation.rules().modules.as_ref().unwrap().slaved_to;
    let master = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(slaved_to, pup)
            .map(|link| link.master)
    };
    let den = of_kind(&b, &simulation, ids, "Den")[0];
    assert_eq!(master(&simulation), Some(den));
    press(&mut simulation, PLAYER, den, RotwkCommand::DebugKill);
    run(&mut simulation, 66);
    let site = of_kind(&b, &simulation, ids, "Den")[0];
    assert!(simulation.state().contains(ids.construction, site));
    run(&mut simulation, 30);
    assert_eq!(
        master(&simulation),
        Some(den),
        "not reclaimed while building"
    );
    run(&mut simulation, 40);
    assert!(
        !simulation.state().contains(ids.construction, site),
        "finished"
    );
    run(&mut simulation, 20);
    assert_eq!(
        master(&simulation),
        Some(site),
        "reclaimed by the rebuilt den"
    );
    assert_eq!(
        of_kind(&b, &simulation, ids, "Pup"),
        vec![pup],
        "and no new pup"
    );
    let at = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.body, pup).unwrap().0.position
    };
    let before = at(&simulation);
    let mut moved = false;
    for _ in 0..120 {
        run(&mut simulation, 1);
        moved |= at(&simulation).minus(before).length() > 1.0;
    }
    assert!(moved, "the reclaimed pup paces about its den again");
}

#[test]
fn a_hole_rebuilds_even_when_getting_built_behavior_sets_neither_flag() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("WorkerlessLair", 400.0, 400.0, PLAYER)]);
    let lair = of_kind(&b, &simulation, ids, "WorkerlessLair")[0];
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    assert_eq!(
        of_kind(&b, &simulation, ids, "WorkerlessLairHole").len(),
        1,
        "one hole where the structure stood"
    );
    run(&mut simulation, 65);
    assert_eq!(
        of_kind(&b, &simulation, ids, "WorkerlessLair").len(),
        1,
        "the structure is back"
    );
}

#[test]
fn a_hole_takes_over_its_structures_geometry_and_footprint() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Lair", 400.0, 400.0, PLAYER)]);
    let lair = of_kind(&b, &simulation, ids, "Lair")[0];
    let lair_shape = *simulation.state().get(ids.shape, lair).unwrap();
    let layer = |simulation: &Simulation<RotwkRules>| {
        let world = simulation.state();
        let clock = world.store(ids.obstacles).unwrap().entities()[0];
        world.get(ids.obstacles, clock).unwrap().clone()
    };
    let lair_records = layer(&simulation).records_of(lair).unwrap();
    press(&mut simulation, PLAYER, lair, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let hole = of_kind(&b, &simulation, ids, "LairHole")[0];
    assert!(
        simulation.state().is_live(lair),
        "the lair is still collapsing"
    );
    let shape = simulation.state().get(ids.shape, hole).unwrap();
    assert_eq!(
        (shape.radius, shape.height),
        (lair_shape.radius, lair_shape.height)
    );
    assert_eq!(
        RotwkRules::geometry_kind(simulation.state(), ids, hole),
        b.kinds.kind("Lair"),
        "the pick test reads the geometry it took over"
    );
    let layer = layer(&simulation);
    assert!(!layer.holds(lair), "the dying lair no longer blocks");
    assert_eq!(layer.records_of(hole), Some(lair_records));
}

#[test]
fn a_spread_formation_places_each_object_on_its_own_ring_spot() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Hoard", 400.0, 400.0, PLAYER)]);
    let hoard = of_kind(&b, &simulation, ids, "Hoard")[0];
    press(&mut simulation, PLAYER, hoard, RotwkCommand::DebugKill);
    run(&mut simulation, 1);
    let hole = of_kind(&b, &simulation, ids, "HoardHole")[0];
    press(&mut simulation, PLAYER, hole, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    let spots: Vec<Vec2> = of_kind(&b, &simulation, ids, "Loot")
        .into_iter()
        .map(|loot| simulation.state().get(ids.body, loot).unwrap().0.position)
        .collect();
    assert_eq!(spots.len(), 5);
    for spot in &spots {
        let out = spot.minus(Vec2::new(400.0, 400.0)).length();
        assert!((29.9..=70.1).contains(&out), "{spot:?} is {out} out");
    }
    let apart = spots.iter().enumerate().any(|(at, a)| {
        spots
            .iter()
            .skip(at + 1)
            .any(|b| a.minus(*b).length() > 5.0)
    });
    assert!(apart, "not all on one spot: {spots:?}");
}

#[test]
fn kept_rubble_turns_its_cells_to_rubble_until_rebuilt() {
    use mint_movement::Passability as _;
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Ruin", 400.0, 400.0, PLAYER)]);
    let ruin = of_kind(&b, &simulation, ids, "Ruin")[0];
    let centre = |simulation: &Simulation<RotwkRules>| {
        let rules = simulation.rules();
        let cell = rules
            .passability()
            .cell_at(Vec2::new(400.0, 400.0))
            .unwrap();
        let world = simulation.state();
        let view = mint_sage_passability::DynamicView {
            grid: &rules.passability().grid,
            counts: crate::obstacles::counts_of(world, ids.obstacles),
            rubble: crate::obstacles::rubble_of(world, ids.obstacles),
            wall_rim: crate::wall_layer::rim_of(world, ids.wall_layer),
            tags: crate::wall_layer::tags_of(world, ids.wall_layer),
            connect: crate::wall_layer::connect_of(world, ids.wall_layer),
            released: crate::wall_layer::released_of(world, ids.wall_layer),
            closed: &[],
            obstacle_pinched: &[],
            ramp_side: crate::wall_layer::side_of(world, ids.wall_layer),
            scaleable: &[],
            barred: &[],
        };
        view.flags(cell)
    };
    let walker = |surfaces: &str| {
        mint_sage_passability::mover_class(
            &surfaces.split(' ').map(str::to_owned).collect::<Vec<_>>(),
            false,
        )
        .unwrap()
    };
    let crosses = |flags: mint_movement::CellFlags, surfaces: &str| {
        !flags.intersects(walker(surfaces).blocked_by)
    };
    assert!(
        !crosses(centre(&simulation), "GROUND RUBBLE"),
        "standing, it blocks"
    );
    press(&mut simulation, PLAYER, ruin, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    assert!(simulation.state().is_live(ruin), "kept as rubble");
    let flags = centre(&simulation);
    assert!(
        crosses(flags, "GROUND RUBBLE"),
        "a rubble walker crosses it"
    );
    assert!(!crosses(flags, "GROUND"), "a ground-only walker does not");
    press(&mut simulation, PLAYER, ruin, RotwkCommand::StartSelfRepair);
    run(&mut simulation, 1);
    let rebuilt = centre(&simulation);
    assert!(
        !crosses(rebuilt, "GROUND RUBBLE"),
        "rebuilding, it blocks again"
    );
    assert!(
        !rebuilt.intersects(mint_sage_passability::RUBBLE),
        "the footprint is obstacle again, not left marked as rubble"
    );
}

#[test]
fn a_rubbled_structure_keeps_only_its_bookend_part() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("BookendRuin", 400.0, 400.0, PLAYER)]);
    let ruin = of_kind(&b, &simulation, ids, "BookendRuin")[0];
    let at = |simulation: &Simulation<RotwkRules>, point: Vec2| {
        let rules = simulation.rules();
        let cell = rules.passability().cell_at(point).unwrap();
        let index =
            usize::try_from(rules.passability().grid.extent().index(cell).unwrap()).unwrap();
        let world = simulation.state();
        (
            crate::obstacles::counts_of(world, ids.obstacles)[index],
            crate::obstacles::rubble_of(world, ids.obstacles)[index],
        )
    };
    let (centre, corner) = (Vec2::new(400.0, 400.0), Vec2::new(418.0, 418.0));
    assert!(at(&simulation, corner).0 > 0, "standing, the box blocks");
    press(&mut simulation, PLAYER, ruin, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    assert!(simulation.state().is_live(ruin), "kept as rubble");
    assert!(at(&simulation, centre).0 > 0, "the Bookend blocks");
    assert_eq!(at(&simulation, corner), (0, 0), "nothing else stands");
}

#[test]
fn a_rubble_rebuild_charges_its_rubble_percent_and_refuses_without_money() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("RuinPriced", 400.0, 400.0, PLAYER),
            ("RuinDear", 800.0, 400.0, PLAYER),
        ],
    );
    let priced = of_kind(&b, &simulation, ids, "RuinPriced")[0];
    let dear = of_kind(&b, &simulation, ids, "RuinDear")[0];
    press(&mut simulation, PLAYER, priced, RotwkCommand::DebugKill);
    press(&mut simulation, PLAYER, dear, RotwkCommand::DebugKill);
    run(&mut simulation, 2);
    assert_eq!(purse(&simulation, ids, PLAYER), 5_000);
    press(
        &mut simulation,
        PLAYER,
        priced,
        RotwkCommand::StartSelfRepair,
    );
    assert!(
        simulation.state().contains(ids.construction, priced),
        "rebuilding"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 3_800, "30% of 4,000");
    let verdict =
        simulation
            .rules()
            .self_repair(simulation.state(), ids, PLAYER, dear, simulation.frame());
    assert_eq!(verdict, crate::rules::SelfRepair::Refused("no money"));
    press(&mut simulation, PLAYER, dear, RotwkCommand::StartSelfRepair);
    assert!(
        !simulation.state().contains(ids.construction, dear),
        "still rubble"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 3_800);
}
