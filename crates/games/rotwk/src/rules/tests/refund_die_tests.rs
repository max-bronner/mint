use super::*;

const DATA: &str = "\
GameData\n  SellPercentage = 50%\nEnd\n\
Upgrade Upgrade_Defiance\n  Type = PLAYER\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Object Market\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = MarketSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 5\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = RefundDie Tag_Refund\n    RefundPercent = 50%\n    \
    UpgradeRequired = Upgrade_Defiance\n    BuildingRequired = ANY +Market\n  End\n  \
  Behavior = StructureCollapseUpdate Tag_Collapse\n    CollapseDamping = .5\n    \
    DestroyObjectWhenDone = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\nEnd\n\
CommandSet MarketSet\n  1 = Buy_Defiance\nEnd\n\
CommandSet BarracksSet\n  1 = Command_Sell\nEnd\n\
CommandButton Buy_Defiance\n  Command = PLAYER_UPGRADE\n  Upgrade = Upgrade_Defiance\nEnd\n\
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
    let seeds: Vec<String> = ["Market", "Barracks"]
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
        .with_sell_percentage(b.data.sell_percentage);
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

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn buy_defiance(simulation: &mut Simulation<RotwkRules>, market: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![market],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::PlayerUpgrade { button: 0 })
        .unwrap();
    run(simulation, 35);
}

#[test]
fn a_death_with_defiance_and_a_market_standing_pays_half_the_cost() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Market", 300.0, 300.0, PLAYER),
            ("Barracks", 500.0, 500.0, PLAYER),
        ],
    );
    let market = of_kind(&b, &simulation, ids, "Market");
    buy_defiance(&mut simulation, market);
    let barracks = of_kind(&b, &simulation, ids, "Barracks");
    let before = purse(&simulation, ids, PLAYER);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![barracks],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    run(&mut simulation, 1);
    assert_eq!(
        purse(&simulation, ids, PLAYER) - before,
        500,
        "50% of the 1000 BuildCost"
    );
}

#[test]
fn a_death_with_no_defiance_pays_nothing() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 500.0, 500.0, PLAYER)]);
    let barracks = of_kind(&b, &simulation, ids, "Barracks");
    let before = purse(&simulation, ids, PLAYER);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![barracks],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    run(&mut simulation, 1);
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        before,
        "UpgradeRequired unheld"
    );
}

#[test]
fn selling_does_not_also_pay_the_refund_die() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Market", 300.0, 300.0, PLAYER),
            ("Barracks", 500.0, 500.0, PLAYER),
        ],
    );
    let market = of_kind(&b, &simulation, ids, "Market");
    buy_defiance(&mut simulation, market);
    let barracks = of_kind(&b, &simulation, ids, "Barracks");
    let before = purse(&simulation, ids, PLAYER);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![barracks],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::Sell).unwrap();
    run(&mut simulation, 2);
    assert_eq!(
        purse(&simulation, ids, PLAYER) - before,
        500,
        "the sale's own refund, once"
    );
}
