use super::*;

const DATA: &str = "\
GameData\n  SellPercentage = 50%\nEnd\n\
Upgrade Upgrade_Walls\n  Type = OBJECT\n  BuildCost = 400\n  BuildTime = 10\nEnd\n\
Upgrade Upgrade_Forge\n  Type = PLAYER\n  BuildCost = 700\n  BuildTime = 10\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 50\n  BuildTime = 5\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 5\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_Walls\n    TriggeredBy = Upgrade_Walls\n    ShowSubObjects = Walls\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    CollapseDamping = .5\n    DestroyObjectWhenDone = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\nEnd\n\
Object Statue\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = StatueSet\n  \
  BuildCost = 600\n  BuildTime = 10\n  RefundValue = 100\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object Shrine\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 800\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 900\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    FXList = INITIAL FX_ShrinePuff\n    \
  DestroyObjectWhenDone = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Tavern\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  KeepSelectableWhenDead = Yes\n  \
  BuildCost = 500\n  BuildTime = 45\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = KeepObjectDie Tag_Rubble\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Rebuild\n    RebuildTimeSeconds = 2\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Forge\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE TEST_DISCOUNT\n  \
  CommandSet = StatueSet\n  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 900\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Obelisk\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = StatueSet\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = CostModifierUpgrade Tag_Discount\n    StartsActive = Yes\n    \
    ObjectFilter = ANY +TEST_DISCOUNT\n    Percentage = -20%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
CommandSet PorterSet\n  1 = Construct_Statue\n  2 = Construct_Shrine\nEnd\n\
CommandButton Construct_Statue\n  Command = DOZER_CONSTRUCT\n  Object = Statue\nEnd\n\
CommandButton Construct_Shrine\n  Command = DOZER_CONSTRUCT\n  Object = Shrine\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldier\n  2 = Buy_Walls\n  3 = Buy_Forge\n  4 = Command_Sell\nEnd\n\
CommandSet StatueSet\n  1 = Command_Sell\nEnd\n\
CommandButton Build_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n\
CommandButton Buy_Walls\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Walls\nEnd\n\
CommandButton Buy_Forge\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_Forge\nEnd\n\
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
    assert!((data.sell_percentage - 0.5).abs() < f32::EPSILON);
    let seeds: Vec<String> = [
        "Barracks", "Statue", "Shrine", "Soldier", "Porter", "Tavern", "Forge", "Obelisk",
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
            let mut spawn = b
                .kinds
                .spawn(
                    &b.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap();
            spawn.producer = b
                .kinds
                .producer(&production, kind, Vec2::new(*x, *y), Facing::EAST);
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
        .with_sell_percentage(b.data.sell_percentage);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
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

fn stamped(simulation: &Simulation<RotwkRules>, ids: Ids) -> usize {
    let world = simulation.state();
    let rubble = crate::obstacles::rubble_of(world, ids.obstacles);
    crate::obstacles::counts_of(world, ids.obstacles)
        .iter()
        .enumerate()
        .filter(|(at, count)| **count > 0 || rubble.get(*at).is_some_and(|r| *r > 0))
        .count()
}

#[test]
fn a_sold_structure_is_refunded_at_once_then_collapses_and_goes() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    assert!(stamped(&simulation, ids) > 0);
    let before = purse(&simulation, ids, PLAYER);
    press(&mut simulation, PLAYER, barracks, RotwkCommand::Sell);
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        before + 500,
        "SellPercentage 50%"
    );
    assert!(simulation.state().contains(ids.doomed, barracks), "killed");
    assert!(
        simulation.state().get(ids.selected, barracks).is_none(),
        "deselected for every player"
    );
    run(&mut simulation, 5);
    assert!(simulation.state().is_live(barracks), "still collapsing");
    run(&mut simulation, 60);
    assert!(!simulation.state().is_live(barracks), "collapsed and gone");
    assert_eq!(stamped(&simulation, ids), 0, "its footprint went with it");
}

#[test]
fn a_sale_refunds_half_of_the_discounted_cost_not_half_of_the_base_cost() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Forge", 200.0, 200.0, PLAYER),
            ("Obelisk", 400.0, 200.0, PLAYER),
        ],
    );
    let forge = of_kind(&b, &simulation, ids, "Forge")[0];
    let before = purse(&simulation, ids, PLAYER);
    press(&mut simulation, PLAYER, forge, RotwkCommand::Sell);
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        before + 400,
        "the obelisk's -20% makes the forge cost 800, so half of it is 400"
    );
}

#[test]
fn a_structure_with_a_refund_value_refunds_that() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Statue", 200.0, 200.0, PLAYER)]);
    let statue = of_kind(&b, &simulation, ids, "Statue")[0];
    let before = purse(&simulation, ids, PLAYER);
    press(&mut simulation, PLAYER, statue, RotwkCommand::Sell);
    assert_eq!(purse(&simulation, ids, PLAYER), before + 100);
}

#[test]
fn selling_refunds_everything_the_structure_was_producing_at_once() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    let before = purse(&simulation, ids, PLAYER);
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::PlayerUpgrade { button: 0 },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), before - 750);
    press(&mut simulation, PLAYER, barracks, RotwkCommand::Sell);
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        before + 500,
        "production refunded in full, the barracks at 50%"
    );
    assert!(simulation
        .state()
        .get(ids.producer, barracks)
        .unwrap()
        .queue
        .is_empty());
    let player =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(simulation
        .state()
        .get(ids.player_upgrades_in_progress, player)
        .is_none_or(|held| held.0.is_empty()));
}

#[test]
fn only_the_owners_structure_sells_once_and_a_refusal_changes_nothing() {
    let b = load();
    let placed = [
        ("Barracks", 200.0, 200.0, OTHER),
        ("Soldier", 300.0, 300.0, PLAYER),
    ];
    let (mut with, ids) = start(&b, &placed);
    let (mut without, _) = start(&b, &placed);
    let enemy_barracks = of_kind(&b, &with, ids, "Barracks")[0];
    let soldier = of_kind(&b, &with, ids, "Soldier")[0];
    assert_eq!(
        with.rules()
            .sell_refusal(with.state(), ids, PLAYER, enemy_barracks),
        Some("not the seller's")
    );
    assert_eq!(
        with.rules()
            .sell_refusal(with.state(), ids, PLAYER, soldier),
        Some("not a structure")
    );
    for (on, command) in [
        (enemy_barracks, RotwkCommand::Sell),
        (soldier, RotwkCommand::Sell),
    ] {
        for sim in [&mut with, &mut without] {
            sim.submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![on],
                },
            )
            .unwrap();
        }
        with.submit(PLAYER, command).unwrap();
    }
    run(&mut with, 3);
    run(&mut without, 3);
    assert_eq!(with.state_hash(), without.state_hash());
    let (mut twice, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    let barracks = of_kind(&b, &twice, ids, "Barracks")[0];
    press(&mut twice, PLAYER, barracks, RotwkCommand::Sell);
    let paid = purse(&twice, ids, PLAYER);
    assert_eq!(
        twice
            .rules()
            .sell_refusal(twice.state(), ids, PLAYER, barracks),
        Some("dead")
    );
    press(&mut twice, PLAYER, barracks, RotwkCommand::Sell);
    assert_eq!(purse(&twice, ids, PLAYER), paid);
}

#[test]
fn a_site_under_construction_is_not_sold() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Porter", 100.0, 100.0, PLAYER)]);
    let porter = of_kind(&b, &simulation, ids, "Porter")[0];
    let statue = b.kinds.kind("Statue").unwrap();
    press(
        &mut simulation,
        PLAYER,
        porter,
        RotwkCommand::Construct {
            kind: statue.0,
            x: 300.0,
            y: 300.0,
            angle: 0.0,
        },
    );
    let mut site = None;
    for _ in 0..600 {
        site = of_kind(&b, &simulation, ids, "Statue").first().copied();
        if site.is_some() {
            break;
        }
        run(&mut simulation, 1);
    }
    let site = site.expect("the porter reached the spot and the site appeared");
    assert!(simulation.state().contains(ids.construction, site));
    assert_eq!(
        simulation
            .rules()
            .sell_refusal(simulation.state(), ids, PLAYER, site),
        Some("still under construction - that is a cancel")
    );
    press(&mut simulation, PLAYER, site, RotwkCommand::Sell);
    assert!(!simulation.state().contains(ids.doomed, site));
}

fn shrine_under_construction(
    b: &Base,
    frames: u32,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity, i64) {
    let (mut simulation, ids) = start(b, &[("Porter", 100.0, 100.0, PLAYER)]);
    let porter = of_kind(b, &simulation, ids, "Porter")[0];
    let before = purse(&simulation, ids, PLAYER);
    let shrine = b.kinds.kind("Shrine").unwrap();
    press(
        &mut simulation,
        PLAYER,
        porter,
        RotwkCommand::Construct {
            kind: shrine.0,
            x: 250.0,
            y: 250.0,
            angle: 0.0,
        },
    );
    let mut site = None;
    for _ in 0..600 {
        site = of_kind(b, &simulation, ids, "Shrine").first().copied();
        if site.is_some() {
            break;
        }
        run(&mut simulation, 1);
    }
    let site = site.expect("the porter reached the spot and the site appeared");
    run(&mut simulation, frames);
    (simulation, ids, porter, site, before)
}

#[test]
fn cancelling_a_started_build_refunds_it_whole_removes_it_at_once_and_frees_the_porter() {
    let b = load();
    let (mut simulation, ids, porter, site, before) = shrine_under_construction(&b, 150);
    assert_eq!(purse(&simulation, ids, PLAYER), before - 800);
    assert!(simulation.state().contains(ids.concealed, porter));
    press(
        &mut simulation,
        PLAYER,
        site,
        RotwkCommand::CancelConstruction,
    );
    assert_eq!(purse(&simulation, ids, PLAYER), before, "100% back");
    assert!(
        !simulation.state().is_live(site),
        "gone at once, no collapse"
    );
    assert!(!simulation.state().contains(ids.concealed, porter));
    assert!(!simulation.state().contains(ids.build_task, porter));
}

#[test]
fn a_cancelled_build_plays_its_collapse_initial_fx() {
    let b = load();
    let (mut simulation, _, _, site, _) = shrine_under_construction(&b, 150);
    let cancelled_on = simulation.frame();
    press(
        &mut simulation,
        PLAYER,
        site,
        RotwkCommand::CancelConstruction,
    );
    let rules = simulation.rules();
    let played: Vec<&str> = rules
        .fx_events(simulation.state(), cancelled_on)
        .iter()
        .filter_map(|event| rules.fx_name(event.list))
        .collect();
    assert_eq!(played, ["FX_ShrinePuff"]);
}

#[test]
fn cancelling_an_upgrade_refunds_it_in_full_and_frees_it() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    let before = purse(&simulation, ids, PLAYER);
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::PlayerUpgrade { button: 0 },
    );
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), before - 1_100);
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::CancelUpgrade {
            button: 0,
            player: true,
        },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), before - 400, "700 back");
    let player =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), PLAYER)
            .unwrap();
    assert!(simulation
        .state()
        .get(ids.player_upgrades_in_progress, player)
        .is_none_or(|held| held.0.is_empty()));
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::CancelUpgrade {
            button: 0,
            player: false,
        },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), before, "400 back");
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::PlayerUpgrade { button: 0 },
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        before - 700,
        "free to queue again"
    );
    let (mut idle, idle_ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    let idle_barracks = of_kind(&b, &idle, idle_ids, "Barracks")[0];
    let (mut control, _) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
    for sim in [&mut idle, &mut control] {
        sim.submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![idle_barracks],
            },
        )
        .unwrap();
    }
    idle.submit(
        PLAYER,
        RotwkCommand::CancelUpgrade {
            button: 0,
            player: false,
        },
    )
    .unwrap();
    run(&mut idle, 2);
    run(&mut control, 2);
    assert_eq!(idle.state_hash(), control.state_hash());
}

#[test]
fn a_sale_replays_identically() {
    let run_once = || {
        let b = load();
        let (mut simulation, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)]);
        let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
        press(
            &mut simulation,
            PLAYER,
            barracks,
            RotwkCommand::QueueUnit { button: 0 },
        );
        press(&mut simulation, PLAYER, barracks, RotwkCommand::Sell);
        let mut hashes = Vec::new();
        for _ in 0..190 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run_once();
    assert_eq!(first, run_once());
    assert!(first.iter().any(|hash| *hash != first[0]));
}

#[test]
fn rubble_stands_until_its_owner_rebuilds_it_over_its_rebuild_time() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Tavern", 200.0, 200.0, PLAYER)]);
    let tavern = of_kind(&b, &simulation, ids, "Tavern")[0];
    press(&mut simulation, PLAYER, tavern, RotwkCommand::DebugKill);
    run(&mut simulation, 300);
    assert!(simulation.state().contains(ids.doomed, tavern), "dead");
    assert!(simulation.state().is_live(tavern), "never removed");
    assert!(stamped(&simulation, ids) > 0, "its footprint stays");
    press(
        &mut simulation,
        PlayerId::new(1),
        tavern,
        RotwkCommand::StartSelfRepair,
    );
    assert!(
        simulation.state().contains(ids.doomed, tavern),
        "not player 1's to rebuild"
    );
    press(
        &mut simulation,
        PLAYER,
        tavern,
        RotwkCommand::StartSelfRepair,
    );
    assert!(
        !simulation.state().contains(ids.doomed, tavern),
        "rebuilding"
    );
    run(&mut simulation, 58);
    assert!(
        simulation.state().contains(ids.construction, tavern),
        "2 s is 60 frames"
    );
    run(&mut simulation, 2);
    assert!(
        !simulation.state().contains(ids.construction, tavern),
        "rebuilt"
    );
    let health = simulation.state().get(ids.health, tavern).unwrap().0;
    assert!((health.current() - health.max()).abs() < f32::EPSILON);
}

mod destroy_when_sold {
    use super::*;

    const DATA: &str = "\
Upgrade Upgrade_Spawn\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 0\nEnd\n\
Object Workshop\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 5\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_Spawn\n  End\n  \
  Behavior = ObjectCreationUpgrade Tag_Spawn\n    TriggeredBy = Upgrade_Spawn\n    \
    ThingToSpawn = Turret\n    Offset = X:20.0 Y:0.0 Z:0.0\n    DestroyWhenSold = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Turret\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 50\n  BuildTime = 5\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 6\n  GeometryHeight = 16\nEnd\n";

    const MOUNTED: &str = "\
Upgrade Upgrade_Spawn\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 0\nEnd\n\
Locomotor Mount\n  Surfaces = GROUND\n  TurnTime = 10000\nEnd\n\
Object Workshop\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 5\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_Spawn\n  End\n  \
  Behavior = ObjectCreationUpgrade Tag_Spawn\n    TriggeredBy = Upgrade_Spawn\n    \
    ThingToSpawn = Turret\n    Offset = X:5.0 Y:0.0 Z:51.0\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Turret\n  Side = Men\n  KindOf = SELECTABLE CAN_ATTACK\n  \
  BuildCost = 50\n  BuildTime = 5\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  LocomotorSet\n    Locomotor = Mount\n    Condition = SET_NORMAL\n    Speed = 0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 6\n  GeometryHeight = 16\nEnd\n";

    fn setup() -> (Simulation<RotwkRules>, Arc<KindTable>, Ids) {
        setup_from(DATA)
    }

    fn setup_from(data: &str) -> (Simulation<RotwkRules>, Arc<KindTable>, Ids) {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            data,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let (data, _) =
            crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
        let (kinds, missing) =
            KindTable::closure(&data, &[String::from("Workshop"), String::from("Turret")]);
        assert!(missing.is_empty(), "{missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let workshop = kinds
            .spawn(
                &data.combat,
                kinds.kind("Workshop").unwrap(),
                Vec2::new(400.0, 400.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let footprint = crate::construction::footprint_of(
            &kinds.get(kinds.kind("Workshop").unwrap()).unwrap().record,
        );
        let map = flat_map(96, 96);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let rules = RotwkRules::new(passability, vec![workshop])
            .with_structure_footprints(vec![(0, footprint)])
            .with_players(2)
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
        let simulation = Simulation::new(5, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        (simulation, kinds, ids)
    }

    fn of_kind(
        kinds: &KindTable,
        simulation: &Simulation<RotwkRules>,
        ids: Ids,
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

    #[test]
    fn a_slave_stands_at_its_offset_inside_the_hosts_footprint() {
        let (mut simulation, kinds, ids) = setup_from(MOUNTED);
        simulation.tick().unwrap();
        let turret = of_kind(&kinds, &simulation, ids, "Turret")[0];
        let at = simulation.state().get(ids.body, turret).unwrap().0.position;
        assert!(
            (at.x - 405.0).abs() < 1e-3 && (at.y - 400.0).abs() < 1e-3,
            "{at:?}"
        );
    }

    #[test]
    fn selling_the_host_removes_the_slave_it_spawned() {
        let (mut simulation, kinds, ids) = setup();
        simulation.tick().unwrap();
        let turrets = of_kind(&kinds, &simulation, ids, "Turret");
        assert_eq!(turrets.len(), 1, "ObjectCreationUpgrade fired once");
        let turret = turrets[0];
        assert!(!simulation.state().contains(ids.doomed, turret));

        let workshop = of_kind(&kinds, &simulation, ids, "Workshop")[0];
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![workshop],
                },
            )
            .unwrap();
        simulation.submit(PLAYER, RotwkCommand::Sell).unwrap();
        simulation.tick().unwrap();

        assert!(simulation.state().contains(ids.doomed, workshop));
        assert!(
            simulation.state().contains(ids.doomed, turret),
            "the slave goes with its sold master"
        );
    }
}
