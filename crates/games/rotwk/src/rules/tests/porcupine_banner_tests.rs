use super::*;

const DATA: &str = "\
ExperienceLevel BannerLevel1\n  TargetNames = Bannerman\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel TroopLevel1\n  TargetNames = SoldierHorde\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel TroopLevel2\n  TargetNames = SoldierHorde\n  RequiredExperience = 7\n  Rank = 2\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Bannerman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    ThisFormationIsTheMainFormation = Yes\n    \
    AlternateFormation = PorcupineHorde\n    BannerCarriersAllowed = Bannerman\n    \
    BannerCarrierMinLevel = 1\n    BannerCarrierPosition = UnitType:Soldier Pos:X:30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
ChildObject PorcupineHorde SoldierHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    IsPorcupineFormation = Yes\n    \
    AlternateFormation = SoldierHorde\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_SoldierHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_SoldierHorde\nEnd\n\
CommandButton Build_SoldierHorde\n  Command = UNIT_BUILD\n  Object = SoldierHorde\nEnd\n";

const MIN_SIZE_DATA: &str = "\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    ThisFormationIsTheMainFormation = Yes\n    \
    AlternateFormation = PorcupineHorde\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
ChildObject PorcupineHorde SoldierHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    IsPorcupineFormation = Yes\n    \
    MinimumHordeSize = 5\n    AlternateFormation = SoldierHorde\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_SoldierHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_SoldierHorde\nEnd\n\
CommandButton Build_SoldierHorde\n  Command = UNIT_BUILD\n  Object = SoldierHorde\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load(data: &str, seeds: &[&str]) -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = seeds.iter().map(|s| (*s).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match, seed: u64) -> (Simulation<RotwkRules>, Ids) {
    let production = m.kinds.production(&m.data);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&m.data.formations, &m.kinds))
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
        .with_teams(vec![None]);
    let simulation = Simulation::new(seed, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn spawn_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids) -> Entity {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "SoldierHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.state().store(ids.horde).unwrap().entities()[0]
}

#[test]
fn the_toggle_carries_the_banner_carrier_into_a_new_slot() {
    let m = load(DATA, &["Camp", "SoldierHorde"]);
    let (mut simulation, ids) = simulation(&m, 11);
    let horde = spawn_horde(&mut simulation, ids);
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
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 7 })
        .unwrap();
    simulation.tick().unwrap();
    let held = simulation.state().get(ids.horde, horde).unwrap().clone();
    let banner_slot = held
        .banner_slot
        .expect("rank 2 must have enlisted a carrier");
    let banner = held
        .members
        .iter()
        .copied()
        .find(|member| {
            simulation
                .state()
                .get(ids.member, *member)
                .is_some_and(|held| held.slot == Some(banner_slot))
        })
        .expect("the carrier must hold its reserved slot");

    simulation
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    simulation.tick().unwrap();

    let held = simulation.state().get(ids.horde, horde).unwrap().clone();
    assert!(
        held.members.contains(&banner),
        "the carrier must still be a member after the swap"
    );
    let new_slot = held
        .banner_slot
        .expect("the new table must still reserve a banner slot");
    assert_eq!(
        simulation
            .state()
            .get(ids.member, banner)
            .and_then(|held| held.slot),
        Some(new_slot),
        "the carrier must hold the new table's own reserved slot, not a plain rank slot"
    );
}

#[test]
fn the_toggle_is_refused_below_the_alternates_minimum_horde_size() {
    let m = load(MIN_SIZE_DATA, &["Camp", "SoldierHorde"]);
    let (mut simulation, ids) = simulation(&m, 12);
    let horde = spawn_horde(&mut simulation, ids);
    let kind_before = simulation.state().get(ids.kind, horde).copied().unwrap();
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
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation.state().get(ids.kind, horde).copied(),
        Some(kind_before),
        "two members under the alternate's MinimumHordeSize of 5 must refuse the swap"
    );
    assert_ne!(
        simulation
            .state()
            .get(ids.stance, horde)
            .map(|stance| stance.0),
        Some(mint_sage_ini::Stance::Porcupine),
        "a refused toggle must not flip the stance either"
    );
}
