use super::*;
use crate::rules::{Ids, RotwkCommand};
use mint_sage_map::{BaseEntry, BasePhase, HeightMapData};
use mint_sim::Entity;

const INI: &str = "\
PlayerTemplate FactionA
  Side = SideA
  PlayableSide = Yes
  Evil = No
  StartingBuilding = ASite
  StartingUnit1 = APorter
  StartingUnitOffset1 = X:0 Y:120 Z:0
  StartingUnit0 = AHorde
  StartingUnitOffset0 = X:150 Y:0 Z:0
End
PlayerTemplate FactionB
  Side = SideB
  PlayableSide = Yes
  Evil = Yes
  StartingBuilding = BKeep
  StartingUnit0 = BPorter
  StartingUnitOffset0 = X:0 Y:10 Z:0
End
PlayerTemplate FactionHidden
  PlayableSide = No
End
PlayerTemplate FactionBooked
  SpellBookMp = ZBook
End
Object ZBook
  KindOf = IMMOBILE UNATTACKABLE
End
Object ASite
  KindOf = STRUCTURE SELECTABLE IMMOBILE BASE_SITE
  PlacementViewAngle = -90
  Body = StructureBody Tag_01
    MaxHealth = 5000
  End
  Behavior = CastleBehavior Tag_02
    CastleToUnpackForFaction = SideA CastleA
    FilterValidOwnedEntries = ANY +STRUCTURE
    InstantUnpack = Yes
  End
  Geometry = BOX
  GeometryMajorRadius = 30
  GeometryMinorRadius = 30
  GeometryHeight = 20
End
Object ACitadel
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = ACitadelSet
  ShroudClearingRange = 300
  Body = StructureBody Tag_01
    MaxHealth = 4000
  End
  Behavior = ProductionUpdate Tag_02
  End
  Behavior = QueueProductionExitUpdate Tag_03
    NaturalRallyPoint = X:60 Y:0 Z:0
  End
  Behavior = AutoDepositUpdate Tag_04
    DepositTiming = 100000
    DepositAmount = 10
  End
  Geometry = BOX
  GeometryMajorRadius = 20
  GeometryMinorRadius = 20
  GeometryHeight = 40
End
Object AProp
  KindOf = IMMOBILE
End
Object APorter
  KindOf = SELECTABLE INFANTRY DOZER
  BuildCost = 100
  BuildTime = 1
  CommandPoints = 0
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 200
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 8
  GeometryHeight = 16
End
Object AHorde
  KindOf = SELECTABLE HORDE
  BuildCost = 300
  BuildTime = 2
  CommandPoints = 60
  ShroudClearingRange = 400
  Behavior = HordeContain Tag_01
    Slots = 4
    RandomOffset = X:5 Y:3
    RankInfo = RankNumber:1 UnitType:ASoldier Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10
  End
End
Object ASoldier
  KindOf = SELECTABLE INFANTRY
  CommandPoints = 15
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 150
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
CommandSet ACitadelSet
  1 = Build_APorter
  2 = Build_AHorde
End
CommandButton Build_APorter
  Command = UNIT_BUILD
  Object = APorter
End
CommandButton Build_AHorde
  Command = UNIT_BUILD
  Object = AHorde
End
Object BKeep
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  CommandSet = BKeepSet
  Body = StructureBody Tag_01
    MaxHealth = 3000
  End
  Behavior = ProductionUpdate Tag_02
    MaxQueueEntries = 2
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 25
  GeometryHeight = 30
End
CommandSet BKeepSet
  1 = Build_BPorter
  2 = Build_BSoldier
  3 = Build_BEliteSoldier
  4 = Build_BRegular
End
CommandButton Build_BPorter
  Command = UNIT_BUILD
  Object = BPorter
End
CommandButton Build_BSoldier
  Command = UNIT_BUILD
  Object = BSoldier
End
CommandButton Build_BEliteSoldier
  Command = UNIT_BUILD
  Object = BEliteSoldier
End
CommandButton Build_BRegular
  Command = UNIT_BUILD
  Object = BRegular
End
Object BSoldier
  KindOf = SELECTABLE INFANTRY
  BuildCost = 20
  BuildTime = 0.1
  CommandPoints = 5
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 80
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Object BEliteSoldier
  KindOf = SELECTABLE INFANTRY
  BuildCost = 20
  BuildTime = 0.1
  CommandPoints = 10000
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 80
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
Object BRegular
  KindOf = SELECTABLE INFANTRY
  BuildCost = 20
  BuildTime = 0.1
  CommandPoints = 5
  ShroudClearingRange = 25
  Body = ActiveBody Tag_01
    MaxHealth = 80
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 6
  GeometryHeight = 16
End
ArmyDefinition TestArmyB
  Side = SideB
  MustUseCommandPointPercentage_Phase1 = 50%
  MustUseCommandPointPercentage_Phase2 = 50%
  MustUseCommandPointPercentage_Phase3 = 50%
  PercentToSave_Rush = 10%
  PercentToSave_MidGame = 10%
  PercentToSave_EndGame = 10%
  PhaseDuration_Rush = 1.0
  PhaseDuration_MidGame = 1.0
  EconomyBuilderMinFarmsOwned = 0
  EconomyBuilderPerFarmValue = 70
  AIEconomyAssigment TestFarm
    TemplateName = BTower
  End
  ArmyMemberDefinition BSoldier_Member
    Unit = BSoldier
    PercentageOfArmyPhase1 = 100.0
    PercentageOfArmyPhase2 = 100.0
    PercentageOfArmyPhase3 = 100.0
  End
  ArmyMemberDefinition BEliteSoldier_Member
    Unit = BEliteSoldier
    PercentageOfArmyPhase1 = 100000.0
    PercentageOfArmyPhase2 = 100000.0
    PercentageOfArmyPhase3 = 100000.0
  End
  ArmyMemberDefinition BRegular_Member
    Unit = BRegular
    PercentageOfArmyPhase1 = 200.0
    PercentageOfArmyPhase2 = 200.0
    PercentageOfArmyPhase3 = 200.0
  End
  HeroBuildOrder = BSoldier
End
ArmyDefinition TestArmyA
  Side = SideA
  MustUseCommandPointPercentage_Phase1 = 50%
  MustUseCommandPointPercentage_Phase2 = 50%
  MustUseCommandPointPercentage_Phase3 = 50%
  PercentToSave_Rush = 10%
  PercentToSave_MidGame = 10%
  PercentToSave_EndGame = 10%
  PhaseDuration_Rush = 1.0
  PhaseDuration_MidGame = 1.0
  EconomyBuilderMinFarmsOwned = 0
  ArmyMemberDefinition AHorde_Member
    Unit = AHorde
    PercentageOfArmyPhase1 = 100.0
    PercentageOfArmyPhase2 = 100.0
    PercentageOfArmyPhase3 = 100.0
  End
End
ChildObject BPorter APorter
  BuildCost = 150
  CommandSet = BPorterSet
End
CommandSet BPorterSet
  1 = Construct_BTower
  2 = Construct_BExpensiveTower
End
CommandButton Construct_BTower
  Command = DOZER_CONSTRUCT
  Object = BTower
End
CommandButton Construct_BExpensiveTower
  Command = DOZER_CONSTRUCT
  Object = BExpensiveTower
End
Object BTower
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  BuildCost = 50
  BuildTime = 0.1
  Body = StructureBody Tag_01
    MaxHealth = 500
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 8
  GeometryHeight = 30
End
Object BExpensiveTower
  KindOf = STRUCTURE SELECTABLE IMMOBILE
  BuildCost = 100000
  BuildTime = 0.1
  Body = StructureBody Tag_01
    MaxHealth = 500
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 8
  GeometryHeight = 30
End
Object ZFlag
  KindOf = CAPTURABLE CAPTUREFLAG IMMOBILE
  Body = ActiveBody Tag_01
    MaxHealth = 100
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 4
  GeometryHeight = 20
End
Object ZTavern
  KindOf = STRUCTURE SELECTABLE IMMOBILE LINKED_TO_FLAG
  Body = StructureBody Tag_01
    MaxHealth = 2000
  End
  Geometry = BOX
  GeometryMajorRadius = 20
  GeometryMinorRadius = 20
  GeometryHeight = 30
End
Object ZStump
  KindOf = IMMOBILE
End
Object ZKeep
  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER
  Body = StructureBody Tag_01
    MaxHealth = 4000
  End
End
AIData TheAIData
  SideInfo SideB
    ResourceGatherersEasy = 1
    ResourceGatherersNormal = 2
    ResourceGatherersHard = 3
  End
End
";

const A: PlayerId = PlayerId::new(0);
const B: PlayerId = PlayerId::new(1);

fn data() -> GameData {
    crate::game::tests::data_from(INI)
}

fn flat() -> HeightMapData {
    HeightMapData {
        width: 96,
        height: 96,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; 96 * 96],
    }
}

fn inert_blend_tile() -> mint_sage_map::BlendTileData {
    mint_sage_map::BlendTileData {
        version: 0,
        width: 0,
        height: 0,
        cell_count: 0,
        tile_index: Vec::new(),
        blend_index: Vec::new(),
        extra_blend_index: Vec::new(),
        cliff_info_index: Vec::new(),
        unknown_cell_byte: None,
        bit_planes: Vec::new(),
        num_bitmap_tiles: 0,
        texture_classes: Vec::new(),
        num_edge_tiles: 0,
        edge_texture_classes: Vec::new(),
        blend_records: Vec::new(),
        cliff_records: Vec::new(),
    }
}

fn inputs(data: &GameData, factions: [&str; 2]) -> Inputs {
    let castle = BaseLayout {
        entries: vec![
            BaseEntry {
                template_name: String::from("ACitadel"),
                offset: [0.0, 0.0],
                angle: 0.0,
                build_order: None,
            },
            BaseEntry {
                template_name: String::from("AProp"),
                offset: [40.0, 0.0],
                angle: 0.0,
                build_order: None,
            },
        ],
        skipped: 0,
        refused: 0,
    };
    let start = |slot: u8, name: &str, at: Vec2| {
        let faction = data.players.get(name).unwrap().clone();
        let castle = (faction.side.as_deref() == Some("SideA"))
            .then(|| (String::from("CastleA"), castle.clone()));
        PlayerStart {
            player: PlayerId::new(slot),
            faction,
            at,
            castle,
            ai_base_candidates: Vec::new(),
        }
    };
    Inputs {
        players: vec![
            start(0, factions[0], Vec2::new(300.0, 300.0)),
            start(1, factions[1], Vec2::new(700.0, 700.0)),
        ],
        ..Inputs::default()
    }
}

fn four_inputs(data: &GameData, teams: [Option<u8>; 4], money: i64) -> Inputs {
    let two = inputs(data, ["FactionA", "FactionB"]);
    let mut players = two.players.clone();
    for (slot, (from, at)) in [(0usize, (300.0, 700.0)), (1, (700.0, 300.0))]
        .into_iter()
        .enumerate()
    {
        let mut extra = two.players[from].clone();
        extra.player = PlayerId::new(u8::try_from(slot + 2).unwrap());
        extra.at = Vec2::new(at.0, at.1);
        players.push(extra);
    }
    let half = crate::handicap::Handicap {
        damage: 0.5,
        health: 0.5,
        ..crate::handicap::Handicap::default()
    };
    Inputs {
        players,
        teams: teams.to_vec(),
        handicaps: vec![
            crate::handicap::Handicap::default(),
            crate::handicap::Handicap::default(),
            crate::handicap::Handicap::default(),
            half,
        ],
        starting_money: Some(money),
        ..Inputs::default()
    }
}

fn four_hashes(data: &GameData, teams: [Option<u8>; 4], money: i64, seed: u64) -> Vec<u64> {
    let map = flat();
    let mut start = build(
        data,
        None,
        &four_inputs(data, teams, money),
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        seed,
        &[PlayerId::new(1), PlayerId::new(3)],
    )
    .unwrap();
    assert_eq!(start.simulation.rules().player_count(), 4);
    let ids = ids(&start);
    let mut hashes = vec![start.simulation.state_hash()];
    for slot in 0..4u8 {
        let player = PlayerId::new(slot);
        let world = start.simulation.state();
        let mine: Vec<Entity> = world
            .store(ids.body)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|e| world.get(ids.owner, *e).is_some_and(|o| o.0 == player))
                    .collect()
            })
            .unwrap_or_default();
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: mine,
            },
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 500.0,
                queue: false,
            },
        ] {
            start.simulation.submit(player, command).unwrap();
        }
    }
    for _ in 0..600 {
        start.simulation.tick().unwrap();
        hashes.push(start.simulation.state_hash());
    }
    hashes
}

fn inputs_with_ai_candidates(
    data: &GameData,
    factions: [&str; 2],
    candidates: Vec<(String, BaseLayout)>,
) -> Inputs {
    let mut built = inputs(data, factions);
    if let Some(b) = built.players.get_mut(1) {
        b.ai_base_candidates = candidates;
    }
    built
}

fn start_with(data: &GameData, factions: [&str; 2], seed: u64) -> Start {
    start_with_ai(data, factions, seed, &[])
}

fn start_with_ai(
    data: &GameData,
    factions: [&str; 2],
    seed: u64,
    ai_players: &[PlayerId],
) -> Start {
    let map = flat();
    build(
        data,
        None,
        &inputs(data, factions),
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        seed,
        ai_players,
    )
    .unwrap()
}

fn ids(start: &Start) -> Ids {
    start.simulation.rules().ids().unwrap()
}

fn placed(start: &Start) -> Vec<(String, PlayerId, Vec2)> {
    let ids = ids(start);
    let world = start.simulation.state();
    world
        .store(ids.kind)
        .map(|store| {
            store
                .iter()
                .map(|(entity, kind)| {
                    (
                        start.kinds.get(*kind).unwrap().record.name.clone(),
                        world.get(ids.owner, entity).unwrap().0,
                        world.get(ids.body, entity).unwrap().0.position,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn hordes_of(start: &Start, player: PlayerId) -> Vec<Entity> {
    let ids = ids(start);
    let world = start.simulation.state();
    world
        .store(ids.horde)
        .map(|store| {
            store
                .entities()
                .iter()
                .copied()
                .filter(|e| world.get(ids.owner, *e).is_some_and(|o| o.0 == player))
                .collect()
        })
        .unwrap_or_default()
}

fn entity_named(start: &Start, name: &str, player: PlayerId) -> Entity {
    let ids = ids(start);
    let world = start.simulation.state();
    let kind = start.kinds.kind(name).unwrap();
    world
        .store(ids.kind)
        .and_then(|store| {
            store.iter().find_map(|(entity, found)| {
                (*found == kind && world.get(ids.owner, entity).is_some_and(|o| o.0 == player))
                    .then_some(entity)
            })
        })
        .unwrap()
}

#[test]
fn the_template_table_is_the_closure_of_the_starts_in_breadth_first_order() {
    let data = data();
    let start = start_with(&data, ["FactionA", "FactionB"], 1);
    assert_eq!(
        start.kinds.names(),
        [
            "ASite",
            "ACitadel",
            "AProp",
            "AHorde",
            "APorter",
            "BKeep",
            "BPorter",
            "ASoldier",
            "BSoldier",
            "BEliteSoldier",
            "BRegular",
            "BTower",
            "BExpensiveTower"
        ]
    );
    let porter = start
        .kinds
        .get(start.kinds.kind("BPorter").unwrap())
        .unwrap();
    assert_eq!(porter.record.depth, 1, "a ChildObject");
    assert_eq!(porter.record.build_cost, Some(150.0), "its own price");
    assert_eq!(porter.record.max_health, Some(200.0), "its parent's body");
    assert!(porter.record.dozer);
}

#[test]
fn each_start_places_its_building_its_castle_and_its_units() {
    let data = data();
    let start = start_with(&data, ["FactionA", "FactionB"], 1);
    let placed = placed(&start);
    let names: Vec<(&str, u8)> = placed
        .iter()
        .map(|(name, owner, _)| (name.as_str(), owner.get()))
        .collect();
    assert_eq!(
        names,
        [
            ("ASite", 0),
            ("ACitadel", 0),
            ("BKeep", 1),
            ("APorter", 0),
            ("BPorter", 1),
            ("AHorde", 0),
            ("ASoldier", 0),
            ("ASoldier", 0),
            ("ASoldier", 0),
            ("ASoldier", 0),
        ],
        "AProp is not a STRUCTURE, so the castle's filter leaves it out"
    );
    let at = |name: &str| placed.iter().find(|(n, _, _)| n == name).unwrap().2;
    assert_eq!(at("ACitadel"), Vec2::new(300.0, 300.0));
    let porter = at("APorter");
    assert!(
        (porter.x - 300.0).abs() < 1e-3 && (porter.y - 180.0).abs() < 1e-3,
        "{porter:?}"
    );
    let b_porter = at("BPorter");
    let (dx, dy) = (b_porter.x - 700.0, b_porter.y - 700.0);
    assert!(
        (dx * dx + dy * dy).sqrt() > 25.0,
        "still inside the keep: {b_porter:?}"
    );
    assert_eq!(hordes_of(&start, A).len(), 1);
    assert!(hordes_of(&start, B).is_empty());
    let limits = |start: &Start| {
        let ids = ids(start);
        let economy = RotwkRules::economy_ids(ids);
        let state = start.simulation.state();
        (
            crate::economy::points_of(state, economy, A).limit(),
            crate::economy::points_of(state, economy, B).limit(),
        )
    };
    assert_eq!(limits(&start), (300, 600));
    let swapped = start_with(&data, ["FactionB", "FactionA"], 1);
    assert_eq!(limits(&swapped), (600, 300));
}

#[test]
fn a_castles_placed_objects_are_its_members() {
    let data = data();
    let start = start_with(&data, ["FactionA", "FactionB"], 1);
    let ids = ids(&start);
    let world = start.simulation.state();
    let site = entity_named(&start, "ASite", A);
    let member = |name: &str, player| {
        world
            .get(ids.castle_member, entity_named(&start, name, player))
            .map(|member| member.castle)
    };
    assert_eq!(member("ACitadel", A), Some(site));
    assert_eq!(member("ASite", A), None);
    assert_eq!(member("APorter", A), None);
    assert_eq!(member("BKeep", B), None, "B's start unpacks no castle");
}

#[test]
fn a_button_delivers_the_whole_horde_it_names_and_charges_its_points() {
    let data = data();
    let mut start = start_with(&data, ["FactionA", "FactionB"], 1);
    let ids = ids(&start);
    let economy = RotwkRules::economy_ids(ids);
    let citadel = entity_named(&start, "ACitadel", A);
    let used =
        |start: &Start| crate::economy::points_of(start.simulation.state(), economy, A).used();
    let purse =
        |start: &Start| crate::economy::purse_of(start.simulation.state(), economy, A).amount();
    start.simulation.tick().unwrap();
    assert_eq!(used(&start), 60, "the starting horde");
    start
        .simulation
        .submit(
            A,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![citadel],
            },
        )
        .unwrap();
    start
        .simulation
        .submit(A, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    start
        .simulation
        .submit(A, RotwkCommand::QueueUnit { button: 7 })
        .unwrap();
    start.simulation.tick().unwrap();
    assert_eq!(
        purse(&start),
        mint_sage_economy::DEFAULT_STARTING_CASH - 300
    );
    for _ in 0..70 {
        start.simulation.tick().unwrap();
    }
    let hordes = hordes_of(&start, A);
    assert_eq!(hordes.len(), 2, "the queued horde was delivered as a horde");
    let delivered = start
        .simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members
        .len();
    assert_eq!(delivered, 4, "every slot, not one member");
    assert_eq!(used(&start), 120, "60 each, off the template");

    start
        .simulation
        .submit(A, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    for _ in 0..40 {
        start.simulation.tick().unwrap();
    }
    let porters = placed(&start)
        .iter()
        .filter(|(name, owner, _)| name == "APorter" && *owner == A)
        .count();
    assert_eq!(porters, 2);
    assert_eq!(used(&start), 120);
}

fn hashes(data: &GameData, factions: [&str; 2], seed: u64) -> Vec<u64> {
    let mut start = start_with(data, factions, seed);
    let ids = ids(&start);
    let producers: Vec<Entity> = start
        .simulation
        .state()
        .store(ids.producer)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let mut hashes = vec![start.simulation.state_hash()];
    for (player, (x, y)) in [(A, (500.0, 200.0)), (B, (600.0, 500.0))] {
        let world = start.simulation.state();
        let mine: Vec<Entity> = world
            .store(ids.body)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|e| world.get(ids.owner, *e).is_some_and(|o| o.0 == player))
                    .collect()
            })
            .unwrap_or_default();
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: producers.clone(),
            },
            RotwkCommand::QueueUnit { button: 1 },
            RotwkCommand::SetSelection {
                replace: true,
                units: mine,
            },
            RotwkCommand::MoveTo { x, y, queue: false },
        ] {
            start.simulation.submit(player, command).unwrap();
        }
    }
    for _ in 0..120 {
        start.simulation.tick().unwrap();
        hashes.push(start.simulation.state_hash());
    }
    hashes
}

#[test]
fn the_skirmish_start_replays_to_the_same_hash_sequence() {
    let data = data();
    let first = hashes(&data, ["FactionA", "FactionB"], 7);
    assert_eq!(first, hashes(&data, ["FactionA", "FactionB"], 7));
    assert_eq!(
        first,
        hashes(&self::data(), ["FactionA", "FactionB"], 7),
        "freshly read data, the same world"
    );
}

#[test]
fn a_four_player_start_replays_to_the_same_hash_sequence() {
    let data = data();
    let teams = [Some(0), Some(1), Some(0), Some(1)];
    let first = four_hashes(&data, teams, 2_000, 7);
    assert_eq!(first, four_hashes(&data, teams, 2_000, 7));
    assert_eq!(first, four_hashes(&self::data(), teams, 2_000, 7));
    assert_ne!(first, four_hashes(&data, teams, 2_500, 7), "the purse");
    assert_ne!(first, four_hashes(&data, teams, 2_000, 8), "the seed");
}

#[test]
fn every_player_starts_with_the_setups_money() {
    let data = data();
    let map = flat();
    let start = build(
        &data,
        None,
        &four_inputs(&data, [None; 4], 1_234),
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[],
    )
    .unwrap();
    assert_eq!(start.simulation.rules().player_count(), 4);
    let ids = ids(&start);
    let economy = RotwkRules::economy_ids(ids);
    for slot in 0..4u8 {
        let purse =
            crate::economy::purse_of(start.simulation.state(), economy, PlayerId::new(slot));
        assert_eq!(purse.amount(), 1_234, "player {slot}");
    }
}

#[test]
fn every_player_gets_the_setups_command_point_factor() {
    let data = data();
    let map = flat();
    let limits = |factor: Option<u32>| {
        let mut inputs = four_inputs(&data, [None; 4], 1_000);
        inputs.command_point_factor = factor;
        let start = build(
            &data,
            None,
            &inputs,
            &inert_blend_tile(),
            |obstacles| {
                Passability::builder(&map, 5.0 / 128.0)
                    .obstacles(obstacles)
                    .build()
                    .map(|p| (p, Vec::new()))
            },
            &|_, _| Vec::new(),
            &|_| Vec::new(),
            &|_| None,
            &|_| Vec::new(),
            &|_| false,
            1,
            &[],
        )
        .unwrap();
        let economy = RotwkRules::economy_ids(ids(&start));
        (0..4u8)
            .map(|slot| {
                crate::economy::points_of(start.simulation.state(), economy, PlayerId::new(slot))
                    .limit()
            })
            .collect::<Vec<i32>>()
    };
    let plain = limits(None);
    assert!(plain.iter().all(|l| *l > 0));
    let tripled: Vec<i32> = plain.iter().map(|l| l * 3).collect();
    assert_eq!(limits(Some(300)), tripled);
}

#[test]
fn a_start_carries_its_own_player_count() {
    let data = data();
    let map = flat();
    let mut inputs = four_inputs(&data, [None; 4], 1_000);
    inputs.players.truncate(3);
    let start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[],
    )
    .unwrap();
    assert_eq!(start.simulation.rules().player_count(), 3);
}

#[test]
fn an_ai_never_chooses_an_ally_as_its_enemy() {
    let data = data();
    let map = flat();
    let run = |teams: Vec<Option<u8>>| {
        let inputs = Inputs {
            teams,
            ..inputs(&data, ["FactionA", "FactionB"])
        };
        let mut start = build(
            &data,
            None,
            &inputs,
            &inert_blend_tile(),
            |obstacles| {
                Passability::builder(&map, 5.0 / 128.0)
                    .obstacles(obstacles)
                    .build()
                    .map(|p| (p, Vec::new()))
            },
            &|_, _| Vec::new(),
            &|_| Vec::new(),
            &|_| None,
            &|_| Vec::new(),
            &|_| false,
            7,
            &[A, B],
        )
        .unwrap();
        for _ in 0..12 {
            start.simulation.tick().unwrap();
        }
        let ids = ids(&start);
        start
            .simulation
            .rules()
            .ai_target_player(start.simulation.state(), ids, B)
    };
    assert_eq!(run(vec![None, None]), Some(A), "enemies");
    assert_eq!(run(vec![Some(2), Some(2)]), None, "allies");
}

#[test]
fn allied_computer_players_share_their_lead_target() {
    let data = data();
    let map = flat();
    let inputs = four_inputs(&data, [None, Some(1), Some(1), None], 1_000);
    let (c, d) = (PlayerId::new(2), PlayerId::new(3));
    let mut start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        7,
        &[B, c, d],
    )
    .unwrap();
    for _ in 0..12 {
        start.simulation.tick().unwrap();
    }
    let ids = ids(&start);
    let rules = start.simulation.rules();
    let target = |player| rules.ai_target_player(start.simulation.state(), ids, player);
    assert!(matches!(target(B), Some(p) if p == A || p == d));
    assert_eq!(target(c), target(B));
}

fn map_cast(data: &GameData) -> crate::map_players::MapCast {
    map_cast_with(data, "")
}

fn map_cast_with(data: &GameData, gaia_faction: &str) -> crate::map_players::MapCast {
    use mint_sage_map::{Dictionary, Object, Side, Value};
    let side = |name: &str, enemies: &str| Side {
        properties: Dictionary::from_pairs(vec![
            ("playerName".into(), Value::Str(name.into())),
            ("playerEnemies".into(), Value::Str(enemies.into())),
            (
                "playerFaction".into(),
                Value::Str(if name == "Gaia" { gaia_faction } else { "" }.into()),
            ),
        ]),
        build_list: Vec::new(),
    };
    let object = |template: &str, owner: &str, x: f32| Object {
        x,
        y: 500.0,
        z: 0.0,
        angle: 0.0,
        flags: 0,
        template_name: template.into(),
        properties: Dictionary::from_pairs(vec![(
            mint_sage_map::ORIGINAL_OWNER_KEY.into(),
            Value::Str(owner.into()),
        )]),
    };
    let sides = [side("", ""), side("Gaia", "Brigands"), side("Brigands", "")];
    let objects = [
        object("ZFlag", "Gaia/teamGaia", 400.0),
        object("ZTavern", "Gaia/teamGaia", 480.0),
        object("ZStump", "Gaia/teamGaia", 560.0),
        object("ZFlag", "Brigands/teamBrigands", 640.0),
        object("ZTavern", "Nowhere/teamNowhere", 720.0),
    ];
    crate::map_players::cast(
        &data.templates,
        &sides,
        &objects,
        &[],
        &[],
        &std::collections::BTreeSet::new(),
    )
}

fn map_start(data: &GameData, seed: u64) -> Start {
    map_start_with(data, seed, map_cast(data))
}

fn map_start_with(data: &GameData, seed: u64, map_cast: crate::map_players::MapCast) -> Start {
    let map = flat();
    let inputs = Inputs {
        map_cast,
        ..inputs(data, ["FactionA", "FactionB"])
    };
    build(
        data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        seed,
        &[],
    )
    .unwrap()
}

#[test]
fn map_entities_are_owned_by_the_side_their_original_owner_names() {
    let data = data();
    let start = map_start(&data, 1);
    let rules = start.simulation.rules();
    let owned: Vec<(String, Option<&str>)> = placed(&start)
        .into_iter()
        .filter(|(name, _, _)| name.starts_with('Z'))
        .map(|(name, owner, _)| (name, rules.map_player_name(owner)))
        .collect();
    assert_eq!(
        owned,
        [
            (String::from("ZFlag"), Some("Gaia")),
            (String::from("ZTavern"), Some("Gaia")),
            (String::from("ZFlag"), Some("Brigands")),
            (String::from("ZTavern"), Some("")),
        ]
    );
    assert_eq!(rules.map_player_name(PlayerId::new(2)), Some(""));
    assert!(rules.hostile(PlayerId::new(3), PlayerId::new(4)));
    assert!(
        !rules.hostile(A, PlayerId::new(3)),
        "no list names a contestant"
    );
}

#[test]
fn a_map_player_owns_its_factions_spell_book() {
    let data = data();
    let start = map_start_with(&data, 1, map_cast_with(&data, "FactionBooked"));
    let rules = start.simulation.rules();
    let gaia = (0..8)
        .map(PlayerId::new)
        .find(|&player| rules.map_player_name(player) == Some("Gaia"))
        .unwrap();
    let ids = ids(&start);
    let world = start.simulation.state();
    let book_owners: Vec<PlayerId> = world
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, kind)| start.kinds.get(**kind).unwrap().record.name == "ZBook")
        .map(|(entity, _)| world.get(ids.owner, entity).unwrap().0)
        .collect();
    assert_eq!(book_owners, [gaia]);
}

#[test]
fn a_start_given_the_maps_main_building_places_no_starting_building() {
    let data = data();
    let map = flat();
    let inputs = Inputs {
        map_cast: crate::map_players::MapCast {
            contestants: Vec::new(),
            entities: vec![crate::map_players::MapEntity {
                object: 0,
                template: String::from("ZKeep"),
                owner: crate::map_players::Relation::Contestant(A),
                at: Vec2::new(360.0, 300.0),
                angle: 0.0,
                z: 0.0,
                script_name: None,
                team: None,
                indestructible: false,
                keys: Default::default(),
            }],
            ..crate::map_players::MapCast::default()
        },
        ..inputs(&data, ["FactionA", "FactionB"])
    };
    let start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[],
    )
    .unwrap();
    let structures: Vec<(String, PlayerId)> = placed(&start)
        .into_iter()
        .filter(|(name, _, _)| ["ASite", "ACitadel", "BKeep", "ZKeep"].contains(&name.as_str()))
        .map(|(name, owner, _)| (name, owner))
        .collect();
    assert_eq!(
        structures,
        [(String::from("BKeep"), B), (String::from("ZKeep"), A)]
    );
}

#[test]
fn a_start_with_map_entities_replays_to_the_same_hash_sequence() {
    let run = |data: &GameData| {
        let mut start = map_start(data, 7);
        let ids = ids(&start);
        let mut hashes = vec![start.simulation.state_hash()];
        for player in [A, B] {
            let world = start.simulation.state();
            let mine: Vec<Entity> = world
                .store(ids.body)
                .map(|store| {
                    store
                        .entities()
                        .iter()
                        .copied()
                        .filter(|e| world.get(ids.owner, *e).is_some_and(|o| o.0 == player))
                        .collect()
                })
                .unwrap_or_default();
            for command in [
                RotwkCommand::SetSelection {
                    replace: true,
                    units: mine,
                },
                RotwkCommand::MoveTo {
                    x: 560.0,
                    y: 520.0,
                    queue: false,
                },
            ] {
                start.simulation.submit(player, command).unwrap();
            }
        }
        for _ in 0..240 {
            start.simulation.tick().unwrap();
            hashes.push(start.simulation.state_hash());
        }
        hashes
    };
    let first = run(&data());
    assert_eq!(first, run(&data()));
}

#[test]
fn a_different_seed_or_faction_changes_the_hash_sequence() {
    let data = data();
    let base = hashes(&data, ["FactionA", "FactionB"], 7);
    assert_ne!(base, hashes(&data, ["FactionA", "FactionB"], 8));
    assert_ne!(base, hashes(&data, ["FactionB", "FactionA"], 7));
}

#[test]
fn resolve_finds_the_start_waypoints_and_names_what_is_missing() {
    let data = data();
    let waypoint = |name: &str, x: f32| Object {
        x,
        y: 50.0,
        z: 0.0,
        angle: 0.0,
        flags: 0,
        template_name: String::from(mint_sage_map::WAYPOINT_TEMPLATE),
        properties: mint_sage_map::Dictionary::from_pairs(vec![(
            String::from(mint_sage_map::WAYPOINT_NAME_KEY),
            mint_sage_map::Value::Str(name.to_owned()),
        )]),
    };
    let objects = vec![
        waypoint("Player_2_Start", 900.0),
        waypoint("Player_1_Start", 100.0),
    ];
    let factions = |a: &str, b: &str| {
        [a, b]
            .iter()
            .enumerate()
            .map(|(i, name)| crate::lobby::ResolvedPlayer {
                slot: i,
                player: PlayerId::new(u8::try_from(i).unwrap()),
                faction: (*name).to_owned(),
                hero_pick: crate::lobby::HeroPick::None,
                hero: None,
                team: None,
                colour: u8::try_from(i).unwrap(),
                handicap: 0,
                start: u8::try_from(i).unwrap(),
                ai: None,
            })
            .collect::<Vec<_>>()
    };
    let inputs = resolve(
        &data,
        &objects,
        |_| None,
        &factions("FactionA", "FactionB"),
        "test map",
        None,
        Vec::new(),
        true,
    )
    .unwrap();
    assert_eq!(inputs.players[0].at, Vec2::new(100.0, 50.0));
    assert_eq!(inputs.players[1].at, Vec2::new(900.0, 50.0));
    assert_eq!(inputs.first_obstacle_id, 2);
    assert!(inputs.players[0].castle.is_none());
    assert!(inputs
        .lines
        .iter()
        .any(|line| line.contains("CastleA") && line.contains("did not load")));

    let unknown = resolve(
        &data,
        &objects,
        |_| None,
        &factions("FactionZ", "FactionB"),
        "test map",
        None,
        Vec::new(),
        true,
    )
    .unwrap_err();
    assert!(
        unknown.contains("FactionZ")
            && unknown.contains("FactionA")
            && !unknown.contains("FactionHidden"),
        "{unknown}"
    );
    let short = resolve(
        &data,
        &objects[1..],
        |_| None,
        &factions("FactionA", "FactionB"),
        "test map",
        None,
        Vec::new(),
        true,
    )
    .unwrap_err();
    assert!(
        short.contains("Player_2_Start") && short.contains("Player_1_Start"),
        "{short}"
    );
    let hidden = resolve(
        &data,
        &objects,
        |_| None,
        &factions("FactionHidden", "FactionB"),
        "test map",
        None,
        Vec::new(),
        true,
    )
    .unwrap();
    assert!(hidden
        .lines
        .iter()
        .any(|line| line.contains("not a PlayableSide")));
}

fn base_entry(template: &str, offset: [f32; 2], priority: i32) -> BaseEntry {
    BaseEntry {
        template_name: String::from(template),
        offset,
        angle: 0.0,
        build_order: Some((priority, BasePhase::One)),
    }
}

fn player_entity(start: &Start, player: PlayerId) -> Entity {
    let ids = ids(start);
    let world = start.simulation.state();
    world
        .store(ids.slot)
        .and_then(|store| {
            store
                .iter()
                .find_map(|(entity, slot)| (slot.0 == player).then_some(entity))
        })
        .expect("every player has a slot entity")
}

fn ai_progress(start: &Start, player: PlayerId) -> crate::skirmish_ai::AiProgress {
    let ids = ids(start);
    let entity = player_entity(start, player);
    start
        .simulation
        .state()
        .get(ids.ai_progress, entity)
        .cloned()
        .expect("player was given AI candidates and so must have AiProgress")
}

#[test]
fn the_plan_skips_an_entry_that_already_stands_and_orders_the_rest_by_priority() {
    let data = data();
    let layout = BaseLayout {
        entries: vec![
            base_entry("BKeep", [0.0, 0.0], 1),
            base_entry("BTower", [80.0, 0.0], 20),
            base_entry("BTower", [100.0, 0.0], 5),
        ],
        skipped: 0,
        refused: 0,
    };
    let inputs = inputs_with_ai_candidates(
        &data,
        ["FactionA", "FactionB"],
        vec![(String::from("TestBase"), layout)],
    );
    let map = flat();
    let mut start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[B],
    )
    .unwrap();

    let progress = ai_progress(&start, B);
    assert_eq!(progress.tasks.len(), 2, "BKeep must be filtered out");

    start.simulation.tick().unwrap();
    let progress = ai_progress(&start, B);
    assert!(
        matches!(
            progress.tasks[0],
            crate::skirmish_ai::AiTaskState::Ordered(_)
        ),
        "the lower-priority-number task must be ordered first: {:?}",
        progress.tasks
    );
    assert_eq!(
        progress.tasks[1],
        crate::skirmish_ai::AiTaskState::Pending,
        "only one structure at a time per B6"
    );
}

#[test]
fn a_refused_spot_is_retried_and_the_second_task_still_completes() {
    let data = data();
    let layout = BaseLayout {
        entries: vec![
            base_entry("BTower", [80.0, 0.0], 5),
            base_entry("BTower", [80.0, 0.0], 20),
        ],
        skipped: 0,
        refused: 0,
    };
    let inputs = inputs_with_ai_candidates(
        &data,
        ["FactionA", "FactionB"],
        vec![(String::from("TestBase"), layout)],
    );
    let map = flat();
    let mut start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[B],
    )
    .unwrap();
    for _ in 0..600 {
        start.simulation.tick().unwrap();
    }
    let progress = ai_progress(&start, B);
    assert!(
        progress
            .tasks
            .iter()
            .all(|state| matches!(state, crate::skirmish_ai::AiTaskState::Built(_))),
        "both towers must finish, the second one at a spiralled offset: {:?}",
        progress.tasks
    );
}

#[test]
fn skirmish_ai_replays_to_the_same_hash_sequence_over_thousands_of_ticks() {
    let data = data();
    let layout = BaseLayout {
        entries: vec![
            base_entry("BTower", [80.0, 0.0], 10),
            base_entry("BTower", [120.0, 40.0], 30),
            base_entry("BTower", [-40.0, 80.0], 50),
        ],
        skipped: 0,
        refused: 0,
    };
    let run = || {
        let inputs = inputs_with_ai_candidates(
            &data,
            ["FactionA", "FactionB"],
            vec![(String::from("TestBase"), layout.clone())],
        );
        let map = flat();
        let mut start = build(
            &data,
            None,
            &inputs,
            &inert_blend_tile(),
            |obstacles| {
                Passability::builder(&map, 5.0 / 128.0)
                    .obstacles(obstacles)
                    .build()
                    .map(|p| (p, Vec::new()))
            },
            &|_, _| Vec::new(),
            &|_| Vec::new(),
            &|_| None,
            &|_| Vec::new(),
            &|_| false,
            42,
            &[B],
        )
        .unwrap();
        let mut hashes = vec![start.simulation.state_hash()];
        for _ in 0..3_000 {
            start.simulation.tick().unwrap();
            hashes.push(start.simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn both_players_ai_controlled_replays_to_the_same_hash_sequence() {
    let data = data();
    let run = || {
        let mut start = start_with_ai(&data, ["FactionA", "FactionB"], 7, &[A, B]);
        let mut hashes = vec![start.simulation.state_hash()];
        for _ in 0..4_000 {
            start.simulation.tick().unwrap();
            hashes.push(start.simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn a_built_task_survives_a_later_tasks_repeated_money_refusal() {
    let data = data();
    let layout = BaseLayout {
        entries: vec![
            base_entry("BTower", [80.0, 0.0], 10),
            base_entry("BExpensiveTower", [120.0, 0.0], 20),
        ],
        skipped: 0,
        refused: 0,
    };
    let inputs = inputs_with_ai_candidates(
        &data,
        ["FactionA", "FactionB"],
        vec![(String::from("TestBase"), layout)],
    );
    let map = flat();
    let mut start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[B],
    )
    .unwrap();
    for _ in 0..300 {
        start.simulation.tick().unwrap();
    }
    let progress = ai_progress(&start, B);
    assert!(
        matches!(progress.tasks[0], crate::skirmish_ai::AiTaskState::Built(_)),
        "the cheap tower must have finished and stayed finished: {:?}",
        progress.tasks
    );
    assert_eq!(
        progress.tasks[1],
        crate::skirmish_ai::AiTaskState::Pending,
        "the unaffordable tower must still be waiting, not silently dropped: {:?}",
        progress.tasks
    );
}

#[test]
fn the_ai_census_reports_a_line_for_the_ai_player_and_none_for_the_human() {
    let data = data();
    let layout = BaseLayout {
        entries: vec![base_entry("BTower", [80.0, 0.0], 10)],
        skipped: 0,
        refused: 0,
    };
    let inputs = inputs_with_ai_candidates(
        &data,
        ["FactionA", "FactionB"],
        vec![(String::from("TestBase"), layout)],
    );
    let map = flat();
    let start = build(
        &data,
        None,
        &inputs,
        &inert_blend_tile(),
        |obstacles| {
            Passability::builder(&map, 5.0 / 128.0)
                .obstacles(obstacles)
                .build()
                .map(|p| (p, Vec::new()))
        },
        &|_, _| Vec::new(),
        &|_| Vec::new(),
        &|_| None,
        &|_| Vec::new(),
        &|_| false,
        1,
        &[B],
    )
    .unwrap();
    let ids = ids(&start);
    let lines = start
        .simulation
        .rules()
        .skirmish_ai_census(start.simulation.state(), ids);
    assert_eq!(lines.len(), 1, "only B has an AiSeed: {lines:?}");
    assert!(lines[0].contains("player 1"), "{lines:?}");
    assert!(lines[0].contains("0 of 1"), "nothing built yet: {lines:?}");
}

#[test]
fn the_army_plan_is_built_from_the_shared_fixtures_army_definition() {
    let data = data();
    let start = start_with_ai(&data, ["FactionA", "FactionB"], 1, &[B]);
    let line = start
        .lines
        .iter()
        .find(|line| line.contains("TestArmyB"))
        .unwrap_or_else(|| panic!("no ArmyPlan report line: {:?}", start.lines));
    assert!(
        line.contains("3 of 3 ArmyMemberDefinition(s) resolved"),
        "{line}"
    );
    assert!(line.contains("3 trainable"), "{line}");
    assert!(line.contains("1 of 1 HeroBuildOrder"), "{line}");
    assert!(
        line.contains("porter target 2"),
        "SideInfo override: {line}"
    );
    assert!(line.contains("unresolved member(s): none"), "{line}");
}

#[test]
fn production_toward_the_declared_percentage_trains_a_real_unit() {
    let data = data();
    let mut start = start_with_ai(&data, ["FactionA", "FactionB"], 7, &[B]);
    let ids = ids(&start);
    let economy = RotwkRules::economy_ids(ids);
    let starting_purse = crate::economy::purse_of(start.simulation.state(), economy, B).amount();
    for _ in 0..600 {
        start.simulation.tick().unwrap();
    }
    let ending_purse = crate::economy::purse_of(start.simulation.state(), economy, B).amount();
    assert!(
        ending_purse < starting_purse,
        "the AI never spent anything: {starting_purse} -> {ending_purse}"
    );
    let world = start.simulation.state();
    let soldiers = world
        .store(ids.kind)
        .into_iter()
        .flat_map(|store| store.iter())
        .filter(|(entity, kind)| {
            start
                .kinds
                .get(**kind)
                .is_some_and(|entry| entry.record.name == "BSoldier")
                && world
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == B)
        })
        .count();
    assert!(soldiers > 0, "no BSoldier ever appeared for player 1");
}

#[test]
fn the_deviation_fallback_trains_the_next_member_when_the_first_is_too_expensive() {
    let data = data();
    let mut start = start_with_ai(&data, ["FactionA", "FactionB"], 7, &[B]);
    let ids = ids(&start);
    for _ in 0..600 {
        start.simulation.tick().unwrap();
    }
    let world = start.simulation.state();
    let count_of = |name: &str| {
        world
            .store(ids.kind)
            .into_iter()
            .flat_map(|store| store.iter())
            .filter(|(entity, kind)| {
                start
                    .kinds
                    .get(**kind)
                    .is_some_and(|entry| entry.record.name == name)
                    && world
                        .get(ids.owner, *entity)
                        .is_some_and(|owner| owner.0 == B)
            })
            .count()
    };
    assert_eq!(
        count_of("BEliteSoldier"),
        0,
        "10,000 command points never fits either alignment's cap"
    );
    assert!(
        count_of("BRegular") > 0,
        "the fallback never reached BRegular - production is stuck on \
             BEliteSoldier, the deadlock this test guards against"
    );
}

#[test]
fn the_ai_trains_porters_up_to_the_sideinfo_target() {
    let data = data();
    let mut start = start_with_ai(&data, ["FactionA", "FactionB"], 3, &[B]);
    let ids = ids(&start);
    for _ in 0..300 {
        start.simulation.tick().unwrap();
    }
    let world = start.simulation.state();
    let porters = world
        .store(ids.kind)
        .into_iter()
        .flat_map(|store| store.iter())
        .filter(|(entity, kind)| {
            start
                .kinds
                .get(**kind)
                .is_some_and(|entry| entry.record.dozer)
                && world
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == B)
                && world.is_live(*entity)
        })
        .count();
    assert_eq!(porters, 2, "SideInfo's target was 2");
}

#[test]
fn army_production_replays_to_the_same_hash_sequence_over_thousands_of_ticks() {
    let data = data();
    let run = || {
        let mut start = start_with_ai(&data, ["FactionA", "FactionB"], 11, &[B]);
        let mut hashes = vec![start.simulation.state_hash()];
        for _ in 0..3_000 {
            start.simulation.tick().unwrap();
            hashes.push(start.simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn a_different_seed_can_change_the_chosen_base() {
    let data = data();
    let candidates = vec![
        (
            String::from("One"),
            BaseLayout {
                entries: vec![base_entry("BTower", [80.0, 0.0], 10)],
                skipped: 0,
                refused: 0,
            },
        ),
        (
            String::from("Two"),
            BaseLayout {
                entries: vec![
                    base_entry("BTower", [80.0, 0.0], 10),
                    base_entry("BTower", [120.0, 0.0], 20),
                ],
                skipped: 0,
                refused: 0,
            },
        ),
        (
            String::from("Three"),
            BaseLayout {
                entries: vec![
                    base_entry("BTower", [80.0, 0.0], 10),
                    base_entry("BTower", [120.0, 0.0], 20),
                    base_entry("BTower", [160.0, 0.0], 30),
                ],
                skipped: 0,
                refused: 0,
            },
        ),
    ];
    let task_count_at = |seed: u64| {
        let inputs = inputs_with_ai_candidates(&data, ["FactionA", "FactionB"], candidates.clone());
        let map = flat();
        let start = build(
            &data,
            None,
            &inputs,
            &inert_blend_tile(),
            |obstacles| {
                Passability::builder(&map, 5.0 / 128.0)
                    .obstacles(obstacles)
                    .build()
                    .map(|p| (p, Vec::new()))
            },
            &|_, _| Vec::new(),
            &|_| Vec::new(),
            &|_| None,
            &|_| Vec::new(),
            &|_| false,
            seed,
            &[B],
        )
        .unwrap();
        ai_progress(&start, B).tasks.len()
    };
    let counts: std::collections::BTreeSet<usize> = (0..8u64).map(task_count_at).collect();
    assert!(
        counts.len() > 1,
        "expected at least two different picks across 8 seeds, got {counts:?}"
    );
}

#[test]
fn a_map_placed_object_with_build_variations_draws_a_variant() {
    const VARIANT_DATA: &str = "\
Object ZVariantFlag
  KindOf = IMMOBILE SELECTABLE
  BuildVariations = ZVariantFlagA ZVariantFlagB
  Body = ActiveBody Tag_01
    MaxHealth = 10
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 4
  GeometryHeight = 10
End
Object ZVariantFlagA
  KindOf = IMMOBILE SELECTABLE
  Body = ActiveBody Tag_01
    MaxHealth = 10
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 4
  GeometryHeight = 10
End
Object ZVariantFlagB
  KindOf = IMMOBILE SELECTABLE
  Body = ActiveBody Tag_01
    MaxHealth = 10
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 4
  GeometryHeight = 10
End
";
    let data = crate::game::tests::data_from(VARIANT_DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZVariantFlag")]);
    assert!(missing.is_empty(), "{missing:?}");
    let production = kinds.production(&data);
    let placed_name = |seed: u64| -> String {
        let mut spawns: Vec<Spawn> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        let mut rng = Pcg32::new(seed);
        let inputs = Inputs {
            players: Vec::new(),
            heroes: Vec::new(),
            starts: Vec::new(),
            castle_layouts: crate::castle::CastleLayouts::new(),
            first_obstacle_id: 0,
            map_scripts: None,
            route_markers: Vec::new(),
            lines: Vec::new(),
            teams: Vec::new(),
            handicaps: Vec::new(),
            starting_money: None,
            command_point_factor: None,
            allow_ring_heroes: false,
            test_seeds: Vec::new(),
            ai_difficulty: Vec::new(),
            difficulty: Default::default(),
            map_cast: crate::map_players::MapCast {
                contestants: Vec::new(),
                named: Vec::new(),
                players: Vec::new(),
                entities: vec![crate::map_players::MapEntity {
                    object: 0,
                    template: String::from("ZVariantFlag"),
                    owner: crate::map_players::Relation::Contestant(A),
                    at: Vec2::new(100.0, 100.0),
                    angle: 0.0,
                    z: 0.0,
                    script_name: None,
                    team: None,
                    indestructible: false,
                    keys: Default::default(),
                }],
                lines: Vec::new(),
            },
            trees: Vec::new(),
        };
        place_map_entities(
            &data,
            &inputs,
            1,
            &kinds,
            &production,
            &mut spawns,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut lines,
            &mut rng,
        );
        let kind = spawns[0].kind.unwrap();
        kinds
            .get(kind)
            .map_or(String::new(), |entry| entry.record.name.clone())
    };
    let seen: std::collections::BTreeSet<String> = (1..=24u64).map(placed_name).collect();
    assert!(
        seen.contains("ZVariantFlagA") && seen.contains("ZVariantFlagB"),
        "expected both variants across 24 seeds, saw {seen:?}"
    );
    assert_eq!(
        placed_name(7),
        placed_name(7),
        "the same seed must draw the same variant every time"
    );
}

#[test]
fn a_map_placed_entity_stamps_its_footprint_on_the_dynamic_layer() {
    const DEN_DATA: &str = "\
Object ZMapDen
  KindOf = STRUCTURE IMMOBILE SELECTABLE
  Body = ActiveBody Tag_01
    MaxHealth = 10
  End
  Geometry = CYLINDER
  GeometryMajorRadius = 30
  GeometryHeight = 10
End
";
    let data = crate::game::tests::data_from(DEN_DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZMapDen")]);
    assert!(missing.is_empty(), "{missing:?}");
    let production = kinds.production(&data);
    let den = |object: usize, z: f32| crate::map_players::MapEntity {
        object,
        template: String::from("ZMapDen"),
        owner: crate::map_players::Relation::Contestant(A),
        at: Vec2::new(100.0, 100.0),
        angle: 0.0,
        z,
        script_name: None,
        team: None,
        indestructible: false,
        keys: Default::default(),
    };
    let inputs = Inputs {
        players: Vec::new(),
        heroes: Vec::new(),
        starts: Vec::new(),
        castle_layouts: crate::castle::CastleLayouts::new(),
        first_obstacle_id: 0,
        map_scripts: None,
        route_markers: Vec::new(),
        lines: Vec::new(),
        teams: Vec::new(),
        handicaps: Vec::new(),
        starting_money: None,
        command_point_factor: None,
        allow_ring_heroes: false,
        test_seeds: Vec::new(),
        ai_difficulty: Vec::new(),
        difficulty: Default::default(),
        map_cast: crate::map_players::MapCast {
            contestants: Vec::new(),
            named: Vec::new(),
            players: Vec::new(),
            entities: vec![den(0, 0.0), den(1, 40.0)],
            lines: Vec::new(),
        },
        trees: Vec::new(),
    };
    let mut spawns: Vec<Spawn> = Vec::new();
    let mut footprints = Vec::new();
    place_map_entities(
        &data,
        &inputs,
        1,
        &kinds,
        &production,
        &mut spawns,
        &mut footprints,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Pcg32::new(1),
    );
    assert_eq!(spawns.len(), 2);
    let stamped: Vec<usize> = footprints.iter().map(|(at, _)| *at).collect();
    assert_eq!(stamped, vec![0], "the grounded den only");
}

#[test]
fn a_map_placed_horde_template_becomes_a_placed_horde() {
    let data = data();
    let (kinds, missing) = KindTable::closure(&data, &[String::from("AHorde")]);
    assert!(missing.is_empty(), "{missing:?}");
    let production = kinds.production(&data);
    let inputs = Inputs {
        players: Vec::new(),
        heroes: Vec::new(),
        starts: Vec::new(),
        castle_layouts: crate::castle::CastleLayouts::new(),
        first_obstacle_id: 0,
        map_scripts: None,
        route_markers: Vec::new(),
        lines: Vec::new(),
        teams: Vec::new(),
        handicaps: Vec::new(),
        starting_money: None,
        command_point_factor: None,
        allow_ring_heroes: false,
        test_seeds: Vec::new(),
        ai_difficulty: Vec::new(),
        difficulty: Default::default(),
        map_cast: crate::map_players::MapCast {
            contestants: Vec::new(),
            named: Vec::new(),
            players: Vec::new(),
            entities: vec![crate::map_players::MapEntity {
                object: 3,
                template: String::from("AHorde"),
                owner: crate::map_players::Relation::Contestant(A),
                at: Vec2::new(300.0, 300.0),
                angle: 0.0,
                z: 0.0,
                script_name: Some(String::from("Camp Guards")),
                team: Some(String::from("PlyrA/teamPlyrA")),
                indestructible: false,
                keys: Default::default(),
            }],
            lines: Vec::new(),
        },
        trees: Vec::new(),
    };
    let mut spawns: Vec<Spawn> = Vec::new();
    let mut placed = Vec::new();
    place_map_entities(
        &data,
        &inputs,
        1,
        &kinds,
        &production,
        &mut spawns,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut placed,
        &mut Vec::new(),
        &mut Pcg32::new(1),
    );
    assert!(spawns.is_empty(), "spawned as a loose object");
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].spawn.count, 4);
    assert_eq!(placed[0].name.as_deref(), Some("Camp Guards"));
    assert_eq!(placed[0].team.as_deref(), Some("PlyrA/teamPlyrA"));
}

fn library_group(name: &str) -> mint_sage_map::ScriptNode {
    mint_sage_map::ScriptNode::Group(mint_sage_map::ScriptGroup {
        name: name.to_owned(),
        active: true,
        is_subroutine: false,
        children: Vec::new(),
    })
}

fn node_names(nodes: &[mint_sage_map::ScriptNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| match node {
            mint_sage_map::ScriptNode::Group(group) => group.name.clone(),
            mint_sage_map::ScriptNode::Script(script) => script.name.clone(),
        })
        .collect()
}

fn library_team(owner: &str, name: &str) -> mint_sage_map::TeamDef {
    mint_sage_map::TeamDef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        singleton: true,
        properties: mint_sage_map::Dictionary::default(),
    }
}

fn link_side(
    side: &str,
    list: &[&str],
    libraries: &[(&str, mint_sage_map::MapLibrary)],
    dest: &mut Vec<mint_sage_map::ScriptNode>,
    team_names: &mut std::collections::BTreeSet<String>,
) -> Vec<mint_sage_map::TeamDef> {
    let read = |name: &str| {
        libraries
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, library)| library.clone())
    };
    let mut teams = Vec::new();
    let mut lines = Vec::new();
    let mut link = LibraryLink {
        side,
        read: &read,
        linked: std::collections::BTreeSet::new(),
        team_names,
        teams: &mut teams,
        lines: &mut lines,
    };
    let list: Vec<String> = list.iter().map(|s| (*s).to_owned()).collect();
    let mut linked = Vec::new();
    link.link(&list, &mut linked, 0);
    *dest = take_over(linked, dest);
    teams
}

fn library_with_side_one(nodes: Vec<mint_sage_map::ScriptNode>) -> mint_sage_map::MapLibrary {
    mint_sage_map::MapLibrary {
        scripts: Some(mint_sage_map::ScriptTree {
            player_slots: vec![vec![library_group("slot 0 is never linked")], nodes],
        }),
        ..mint_sage_map::MapLibrary::default()
    }
}

#[test]
fn the_sides_own_list_goes_first_then_its_libraries_in_list_order() {
    let libraries = [
        (
            "a",
            library_with_side_one(vec![library_group("A1"), library_group("Own")]),
        ),
        ("b", library_with_side_one(vec![library_group("B1")])),
    ];
    let mut dest = vec![library_group("Own")];
    link_side(
        "PlyrElves",
        &["a", "b"],
        &libraries,
        &mut dest,
        &mut Default::default(),
    );
    assert_eq!(node_names(&dest), ["Own", "A1", "B1"]);
}

fn named_script(name: &str, comment: &str) -> mint_sage_map::ScriptNode {
    mint_sage_map::ScriptNode::Script(mint_sage_map::Script {
        name: name.to_owned(),
        comment: comment.to_owned(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot: false,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions: Vec::new(),
        actions: Vec::new(),
        actions_false: Vec::new(),
        sequence: Default::default(),
    })
}

fn group_of(name: &str, children: Vec<mint_sage_map::ScriptNode>) -> mint_sage_map::ScriptNode {
    mint_sage_map::ScriptNode::Group(mint_sage_map::ScriptGroup {
        name: name.to_owned(),
        active: true,
        is_subroutine: false,
        children,
    })
}

fn tree_of(nodes: &[mint_sage_map::ScriptNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            mint_sage_map::ScriptNode::Script(script) => {
                format!("{}/{}", script.name, script.comment)
            }
            mint_sage_map::ScriptNode::Group(group) => {
                format!("{}{{{}}}", group.name, tree_of(&group.children))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_sides_own_script_overrides_a_same_named_library_script_nested_in_a_group() {
    let libraries = [(
        "a",
        library_with_side_one(vec![group_of(
            "Folder",
            vec![
                named_script("Override", "library"),
                named_script("Keep", "library"),
            ],
        )]),
    )];
    let mut dest = vec![named_script("Override", "own")];
    link_side(
        "PlyrElves",
        &["a"],
        &libraries,
        &mut dest,
        &mut Default::default(),
    );
    assert_eq!(tree_of(&dest), "Override/own Folder{Keep/library}");
}

#[test]
fn of_two_libraries_the_earlier_listed_overrides_and_a_name_repeated_in_one_list_keeps_one() {
    let libraries = [
        (
            "a",
            library_with_side_one(vec![named_script("Shared", "a")]),
        ),
        (
            "b",
            library_with_side_one(vec![
                named_script("Shared", "b"),
                group_of("G", vec![named_script("Twice", "nested")]),
                named_script("Twice", "top"),
                named_script("Last", "first"),
                named_script("Last", "second"),
            ]),
        ),
    ];
    let mut dest = Vec::new();
    link_side(
        "PlyrElves",
        &["a", "b"],
        &libraries,
        &mut dest,
        &mut Default::default(),
    );
    assert_eq!(tree_of(&dest), "Shared/a G{Twice/nested} Last/second");
}

#[test]
fn a_linked_librarys_owned_teams_join_the_linking_side_unless_the_name_is_taken() {
    let mut library = library_with_side_one(Vec::new());
    library.teams = vec![
        library_team("", "teamNeutral"),
        library_team("Player", "End Team"),
        library_team("Player", "Taken"),
    ];
    let mut team_names = std::collections::BTreeSet::from(["Taken".to_owned()]);
    let teams = link_side(
        "PlyrElves",
        &["a"],
        &[("a", library)],
        &mut Vec::new(),
        &mut team_names,
    );
    let owned: Vec<(String, String)> = teams.into_iter().map(|t| (t.owner, t.name)).collect();
    assert_eq!(owned, [("PlyrElves".to_owned(), "End Team".to_owned())]);
}
