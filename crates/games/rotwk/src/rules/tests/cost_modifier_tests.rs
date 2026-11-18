use super::*;

const DATA: &str = "\
Object Soldiers\n  Side = Men\n  KindOf = INFANTRY TEST_DISCOUNT_KIND SELECTABLE\n  \
  BuildCost = 250\n  BuildTime = 5\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 10\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\nEnd\n\
Object TestStatue\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = StatueSet\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = CostModifierUpgrade Tag_Discount\n    StartsActive = Yes\n    \
    ObjectFilter = ANY +TEST_DISCOUNT_KIND\n    \
    Percentage = -4%\n    Percentage = -8%\n    Percentage = -12%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldiers\n  2 = Command_Sell\nEnd\n\
CommandSet StatueSet\n  1 = Command_Sell\nEnd\n\
CommandButton Build_Soldiers\n  Command = UNIT_BUILD\n  Object = Soldiers\nEnd\n\
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
    let seeds: Vec<String> = ["Barracks", "TestStatue", "Soldiers"]
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

const SELL_TICKS: u32 = 180;

#[test]
fn a_kth_standing_statue_selects_the_kth_percentage_and_selling_restores_it() {
    let b = load();
    let (mut simulation, ids) = start(
        &b,
        &[
            ("Barracks", 200.0, 200.0, PLAYER),
            ("TestStatue", 260.0, 200.0, PLAYER),
            ("TestStatue", 260.0, 260.0, PLAYER),
            ("TestStatue", 320.0, 200.0, PLAYER),
        ],
    );
    let barracks = of_kind(&b, &simulation, ids, "Barracks")[0];
    let statues = of_kind(&b, &simulation, ids, "TestStatue");
    assert_eq!(statues.len(), 3);

    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        220,
        "three standing statues select the third Percentage entry (-12%)"
    );

    press(&mut simulation, PLAYER, statues[0], RotwkCommand::Sell);
    run(&mut simulation, SELL_TICKS);
    assert!(!simulation.state().is_live(statues[0]), "sold");
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        230,
        "two standing statues select the second entry (-8%) once one is sold"
    );

    press(&mut simulation, PLAYER, statues[1], RotwkCommand::Sell);
    run(&mut simulation, SELL_TICKS);
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        240,
        "one standing statue selects the first entry (-4%)"
    );

    press(&mut simulation, PLAYER, statues[2], RotwkCommand::Sell);
    run(&mut simulation, SELL_TICKS);
    press(
        &mut simulation,
        PLAYER,
        barracks,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        last_paid(&simulation, ids, barracks),
        250,
        "no statue standing restores the base price"
    );
}
