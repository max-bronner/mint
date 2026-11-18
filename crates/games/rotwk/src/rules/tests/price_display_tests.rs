use super::*;

const DATA: &str = "\
Object Soldiers\n  Side = Men\n  KindOf = INFANTRY DISCOUNT_KIND SELECTABLE\n  \
  BuildCost = 250\n  BuildTime = 5\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 10\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\nEnd\n\
Object Statue\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = StatueSet\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = CostModifierUpgrade Tag_Discount\n    StartsActive = Yes\n    \
    ObjectFilter = ANY +DISCOUNT_KIND\n    Percentage = -4%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object Shed\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE DISCOUNT_KIND\n  \
  BuildCost = 100\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 200\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 40\n  GeometryMinorRadius = 40\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldiers\nEnd\n\
CommandSet StatueSet\n  1 = Command_Sell\nEnd\n\
CommandSet PorterSet\n  1 = Build_Shed\nEnd\n\
CommandButton Build_Soldiers\n  Command = UNIT_BUILD\n  Object = Soldiers\n  InPalantir = Yes\nEnd\n\
CommandButton Command_Sell\n  Command = SELL\n  InPalantir = Yes\nEnd\n\
CommandButton Build_Shed\n  Command = DOZER_CONSTRUCT\n  Object = Shed\nEnd\n";

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
    let seeds: Vec<String> = ["Barracks", "Statue", "Soldiers", "Porter", "Shed"]
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

fn start(
    b: &Base,
    placed: &[(&str, f32, f32, PlayerId)],
    production_cost_change: Vec<f32>,
) -> (Simulation<RotwkRules>, Ids) {
    let production = b.kinds.production(&b.data);
    let power_names = crate::powers::PowerNames::build(b.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&b.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
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
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(b.kinds.upgrades(&b.data), Arc::clone(&b.data.upgrade_bits))
        .with_production_cost_change(production_cost_change)
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(5, rules).unwrap();
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

fn last_paid(simulation: &Simulation<RotwkRules>, ids: Ids, structure: Entity) -> i64 {
    simulation
        .state()
        .get(ids.producer, structure)
        .unwrap()
        .queue
        .entries()
        .last()
        .unwrap()
        .paid
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

#[test]
fn the_command_bar_shows_exactly_what_a_press_would_charge() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 200.0, 200.0, PLAYER),
            ("Statue", 260.0, 200.0, PLAYER),
        ],
        Vec::new(),
    );
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    let displayed = {
        let rules = simulation.rules();
        let world = simulation.state();
        let counts = rules.cost_modifier_counts(world, ids, PLAYER);
        let cost = |k: crate::unit::Kind| rules.build_price_from_counts(&counts, PLAYER, k);
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
            released_powers: &[],
            command_set: None,
            sell_refusal: None,
            self_repair: None,
            gate: None,
            cancel_refusal: None,
            under_construction: false,
            power_clock: &|_| None,
            auto_cast_on: &|_| false,
            weapon_set_raised: &|_| false,
            foreign_owner: false,
        };
        let bar = crate::command_bar::resolve_command_bar(
            &b.kinds,
            &b.data.command_sets,
            &b.data.command_buttons,
            Some(b.kinds.kind("Barracks").unwrap()),
            &ctx,
        );
        bar.ring[0].as_ref().and_then(|slot| slot.cost)
    };
    assert_eq!(
        displayed,
        Some(240),
        "one standing statue discounts Soldiers' BuildCost = 250 by 4%"
    );

    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        displayed.unwrap(),
        "the queue must charge exactly what the bar showed"
    );
}

#[test]
fn a_porters_construct_price_drops_under_a_building_price_modifier() {
    let b = load();
    let charged = |placed: &[(&str, f32, f32, PlayerId)]| {
        let (mut simulation, ids) = start(&b, placed, Vec::new());
        let porter = of_kind(&b, &simulation, ids, "Porter")[0];
        let before = purse(&simulation, ids, PLAYER);
        press(
            &mut simulation,
            PLAYER,
            porter,
            RotwkCommand::Construct {
                kind: b.kinds.kind("Shed").unwrap().0,
                x: 400.0,
                y: 400.0,
                angle: 0.0,
            },
        );
        before - purse(&simulation, ids, PLAYER)
    };
    let base = charged(&[("Porter", 200.0, 200.0, PLAYER)]);
    let discounted = charged(&[
        ("Porter", 200.0, 200.0, PLAYER),
        ("Statue", 260.0, 200.0, PLAYER),
    ]);
    assert_eq!(base, 100, "Shed's own BuildCost, no discount standing");
    assert_eq!(
        discounted, 96,
        "one standing statue discounts Shed's BuildCost by 4% too"
    );
}

#[test]
fn a_players_production_cost_change_discounts_its_own_queue_price() {
    let b = load();
    let (mut simulation, ids) = start(&b, &[("Barracks", 200.0, 200.0, PLAYER)], vec![-0.2]);
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        200,
        "slot 0's ProductionCostChange of -20% on Soldiers' BuildCost = 250"
    );
}
