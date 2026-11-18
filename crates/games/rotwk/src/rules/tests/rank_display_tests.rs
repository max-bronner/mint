use super::*;
use crate::experience::RankDisplay;

const SHORT: u64 = 90;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Rock\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Wisp Rock\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 3000\n    MaxLifetime = 3000\n  End\nEnd\n\
ChildObject Lamp Rock\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 6000\n    MaxLifetime = 6000\n  End\nEnd\n\
ChildObject Mote Lamp\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\nEnd\n\
Object Drift\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 3000\n    MaxLifetime = 3000\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Mote Position:X:0 Y:20 Position:X:0 Y:-20\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Drift\nEnd\n\
CommandButton Build_Drift\n  Command = UNIT_BUILD\n  Object = Drift\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, [Entity; 5]) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Drift", "Wisp", "Lamp", "Rock"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("Wisp", 200.0),
        ("Wisp", 300.0),
        ("Lamp", 400.0),
        ("Rock", 500.0),
    ]
    .into_iter()
    .map(|(name, x)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 200.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let singles = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Drift")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 500.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = *simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .unwrap();
    let all = [singles[0], singles[1], singles[2], singles[3], horde];
    (simulation, ids, all)
}

fn shown(simulation: &Simulation<RotwkRules>, selected: &[Entity]) -> Option<RankDisplay> {
    simulation
        .rules()
        .rank_display(simulation.state(), selected, simulation.frame())
}

fn time_left(simulation: &Simulation<RotwkRules>, selected: &[Entity]) -> Option<f32> {
    match shown(simulation, selected)? {
        RankDisplay::TimeRemaining(left) => Some(left),
        RankDisplay::Level(_) => None,
    }
}

fn run_to(simulation: &mut Simulation<RotwkRules>, frame: u64) {
    while simulation.frame().get() < frame {
        simulation.tick().unwrap();
    }
}

#[test]
fn a_timed_object_shows_the_share_of_its_lifetime_left_and_an_untimed_one_nothing() {
    let (mut simulation, ids, [wisp, _, _, rock, _]) = scene();
    let lifetime = *simulation.state().get(ids.lifetime, wisp).unwrap();
    let set_at = lifetime.set_at.unwrap().get();
    assert_eq!(lifetime.dies_at.get(), set_at + SHORT);
    assert_eq!(shown(&simulation, &[rock]), None);

    run_to(&mut simulation, set_at + SHORT / 3);
    let early = time_left(&simulation, &[wisp]).unwrap();
    assert!((early - 2.0 / 3.0).abs() < 1e-6, "a third gone: {early}");
    run_to(&mut simulation, set_at + SHORT - 9);
    let late = time_left(&simulation, &[wisp]).unwrap();
    assert!((late - 0.1).abs() < 1e-6, "a tenth left: {late}");
}

#[test]
fn a_selection_shows_its_time_only_while_every_object_agrees() {
    let (mut simulation, ids, [wisp, twin, lamp, rock, _]) = scene();
    let set_at = simulation
        .state()
        .get(ids.lifetime, wisp)
        .and_then(|lifetime| lifetime.set_at)
        .unwrap()
        .get();
    run_to(&mut simulation, set_at + SHORT / 2);
    assert_eq!(time_left(&simulation, &[wisp, twin]), Some(0.5));
    assert_eq!(time_left(&simulation, &[lamp]), Some(0.75));
    assert_eq!(shown(&simulation, &[wisp, lamp]), None);
    assert_eq!(shown(&simulation, &[wisp, rock]), None);
    assert_eq!(shown(&simulation, &[rock, wisp]), None);
}

#[test]
fn a_horde_members_time_is_its_hordes_own() {
    let (mut simulation, ids, [.., horde]) = scene();
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let on_horde = *simulation.state().get(ids.lifetime, horde).unwrap();
    let on_member = *simulation.state().get(ids.lifetime, member).unwrap();
    let set_at = on_horde.set_at.unwrap().get();
    assert_eq!(on_horde.dies_at.get(), set_at + SHORT);
    assert!(
        on_member.dies_at > on_horde.dies_at,
        "the fixture's member outlives its horde's span"
    );
    run_to(&mut simulation, set_at + SHORT / 2);
    assert_eq!(time_left(&simulation, &[member]), Some(0.5));
    assert_eq!(time_left(&simulation, &[horde]), Some(0.5));
}
