use super::*;

const DATA: &str = "\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 10\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 10\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  GeometryHeight = 40\nEnd\n\
Object Soldiers\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 10\n  BuildTime = 1\n  BuildVariations = SoldiersA SoldiersB\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldiersA\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldiersB\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldiers\nEnd\n\
CommandButton Build_Soldiers\n  Command = UNIT_BUILD\n  Object = Soldiers\nEnd\n";

const HORDE_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object SoldiersHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  \
  BuildCost = 10\n  BuildTime = 1\n  CommandPoints = 5\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 6\n    \
    RankInfo = RankNumber:1 UnitType:Soldiers Position:X:10 Y:-15 Position:X:10 Y:-5 \
Position:X:10 Y:5 Position:X:10 Y:15 Position:X:0 Y:-5 Position:X:0 Y:5\n  End\nEnd\n\
CommandButton Build_SoldiersHorde\n  Command = UNIT_BUILD\n  Object = SoldiersHorde\nEnd\n";

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
    let seeds: Vec<String> = ["Barracks", "Soldiers"]
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

fn start(b: &Base, seed: u64) -> (Simulation<RotwkRules>, Ids) {
    let production = b.kinds.production(&b.data);
    let kind = b.kinds.kind("Barracks").unwrap();
    let mut spawn = b
        .kinds
        .spawn(
            &b.data.combat,
            kind,
            Vec2::new(200.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    spawn.producer = b
        .kinds
        .producer(&production, kind, spawn.position, Facing::EAST);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_players(1)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(b.kinds.upgrades(&b.data), Arc::clone(&b.data.upgrade_bits));
    let simulation = Simulation::new(seed, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
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

fn built_variant(b: &Base, seed: u64) -> &'static str {
    let (mut simulation, ids) = start(b, seed);
    let barracks = of_kind(b, &simulation, ids, "Barracks")[0];
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
    for _ in 0..200 {
        simulation.tick().unwrap();
        if !of_kind(b, &simulation, ids, "SoldiersA").is_empty() {
            return "SoldiersA";
        }
        if !of_kind(b, &simulation, ids, "SoldiersB").is_empty() {
            return "SoldiersB";
        }
    }
    panic!("no unit appeared within 200 frames");
}

#[test]
fn a_press_draws_both_variants_across_seeds_and_the_same_seed_always_the_same_one() {
    let b = load();
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=24u64 {
        seen.insert(built_variant(&b, seed));
    }
    assert!(
        seen.contains("SoldiersA") && seen.contains("SoldiersB"),
        "expected both variants across 24 seeds, saw {seen:?}"
    );
    let first = built_variant(&b, 7);
    let second = built_variant(&b, 7);
    assert_eq!(
        first, second,
        "the same seed must draw the same variant every time"
    );
}

#[test]
fn a_fresh_horde_mixes_build_variations_across_its_own_members() {
    let text = format!("{DATA}{HORDE_DATA}").replace(
        "CommandSet BarracksSet\n  1 = Build_Soldiers\nEnd\n",
        "CommandSet BarracksSet\n  1 = Build_Soldiers\n  2 = Build_SoldiersHorde\nEnd\n",
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Barracks", "SoldiersHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(1)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "SoldiersHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 300.0,
                facing: 0.0,
            },
        )
        .unwrap();
    for _ in 0..2 {
        simulation.tick().unwrap();
    }
    let a = kinds.kind("SoldiersA").unwrap();
    let b = kinds.kind("SoldiersB").unwrap();
    let world = simulation.state();
    let horde = world.store(ids.horde).unwrap().entities()[0];
    let members = world.get(ids.horde, horde).unwrap().members.clone();
    assert_eq!(members.len(), 6, "every slot filled");
    let kinds_seen: std::collections::BTreeSet<u16> = members
        .iter()
        .map(|member| world.get(ids.kind, *member).unwrap().0)
        .collect();
    assert!(
        kinds_seen.contains(&a.0) && kinds_seen.contains(&b.0),
        "a 6-member horde at this seed must mix both variants, saw {kinds_seen:?}"
    );
}
