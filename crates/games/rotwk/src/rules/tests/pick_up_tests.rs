use super::*;

const DATA: &str = "\
SpecialPower ZGrabPower\n  Enum = SPECIAL_GRAB_PASSENGER\n  ReloadTime = 0\nEnd\n\
Object ZGiant\n  Side = Wild\n  KindOf = SELECTABLE\n  CommandSet = ZGiantSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GrabPassengerSpecialPower Tag_Grab\n    SpecialPowerTemplate = ZGrabPower\n    \
    UpdateModuleStartsAttack = Yes\n    AllowTree = Yes\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_GrabUpdate\n    SpecialPowerTemplate = ZGrabPower\n    \
    StartAbilityRange = 8.0\n    UnpackTime = 300\n    PreparationTime = 500\n    PersistentPrepTime = 300\n    \
    PackTime = 1000\n    \
    GrabPassengerAnimAndDuration = AnimState:EATING AnimTime:3000 TriggerTime:1400\n    \
    GrabPassengerHealGainPercent = 10.0\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +CLUB +ORC\n    ManualPickUpFilter = ANY +CLUB +ROCK_VENDOR -ORC\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n    AllowNeutralInside = Yes\n    AllowAlliesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
SpecialPower ZChunkPower\n  Enum = SPECIAL_GRAB_CHUNK\n  ReloadTime = 0\nEnd\n\
Object ZChunkGiant\n  Side = Wild\n  KindOf = SELECTABLE\n  CommandSet = ZChunkSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GrabPassengerSpecialPower Tag_Grab\n    SpecialPowerTemplate = ZChunkPower\n    \
    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_GrabUpdate\n    SpecialPowerTemplate = ZChunkPower\n    \
    StartAbilityRange = 8.0\n    UnpackTime = 300\n    PreparationTime = 500\n    PersistentPrepTime = 300\n    \
    PersistentCount = 2\n    PackTime = 1000\n    ParalyzeDurationWhenCompleted = 1000\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    PassengerFilter = ANY +CLUB\n    \
    ManualPickUpFilter = ANY +CHUNK_VENDOR\n    Slots = 2\n    \
    AllowEnemiesInside = Yes\n    AllowNeutralInside = Yes\n    AllowAlliesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Armor ZStone\n  Armor = DEFAULT 100%\n  Armor = STRUCTURAL 50%\nEnd\n\
Object ZRockVendor\n  Side = Wild\n  KindOf = ROCK_VENDOR IMMOBILE SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    GrabObject = ZRock\n    GrabDamage = 40\n    \
    GrabOffset = X:16 Y:0\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZStone\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZChunkVendor\n  Side = Wild\n  KindOf = CHUNK_VENDOR IMMOBILE SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n    GrabObject = ZRock\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = ZStone\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZRock\n  Side = Wild\n  KindOf = ROCK\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 3\n  GeometryHeight = 6\nEnd\n\
CommandSet ZChunkSet\n  1 = Command_ZChunk\nEnd\n\
CommandButton Command_ZChunk\n  Command = SPECIAL_POWER\n  SpecialPower = ZChunkPower\n  \
  Options = NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT NEED_TARGET_ALLY_OBJECT\nEnd\n\
ChildObject ZHungryGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_Hunger\n    ScanDelayTime = 1000\n    ScanDistance = 200\n    \
    EatObjectEntry = MyHealth:99% TargetHealth:100% Filter:NONE +ORC\n  End\nEnd\n\
ChildObject ZButtonGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_ButtonScan\n    ScanDistance = 200\n    PickUpFilter = NONE +ORC\n    \
    RunFromButton = Yes\n    RunFromButtonNumber = 1\n  End\nEnd\n\
ChildObject ZTrollGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_TreeHunt\n    ScanDistance = 200\n    PickUpFilter = NONE +TREE\n  End\nEnd\n\
ChildObject ZButtonTroll ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_ButtonTreeHunt\n    ScanDelayTime = 0\n    ScanDistance = 200\n    \
    PickUpFilter = NONE +TREE\n    RunFromButton = Yes\n    RunFromButtonNumber = 2\n  End\nEnd\n\
Object ZLog\n  Side = Wild\n  KindOf = CLUB SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZOptTree\n  Side = Wild\n  KindOf = CLUB TREE SHRUBBERY IMMOBILE SELECTABLE\n  IsGrabbable = Yes\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZGrunt\n  Side = Wild\n  KindOf = ORC INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ZGiantSet\n  1 = Command_ZGrab\n  2 = Command_ZGrabAt\nEnd\n\
CommandButton Command_ZGrab\n  Command = SPECIAL_POWER\n  SpecialPower = ZGrabPower\n  \
  Options = NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT NEED_TARGET_ALLY_OBJECT\nEnd\n\
CommandButton Command_ZGrabAt\n  Command = SPECIAL_POWER\n  SpecialPower = ZGrabPower\n  \
  Options = NEED_TARGET_POS\nEnd\n\
SpecialPower ZKillPower\n  Enum = SPECIAL_GRAB_CHUNK\n  ReloadTime = 0\nEnd\n\
Object ZKillGiant\n  Side = Wild\n  KindOf = SELECTABLE\n  CommandSet = ZKillSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GrabPassengerSpecialPower Tag_Grab\n    SpecialPowerTemplate = ZKillPower\n    \
    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_GrabUpdate\n    SpecialPowerTemplate = ZKillPower\n    \
    StartAbilityRange = 8.0\n    UnpackTime = 300\n    PreparationTime = 500\n    PersistentPrepTime = 300\n    \
    PersistentCount = 2\n    PackTime = 1000\n    UnpackingVariation = 2\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    PassengerFilter = ANY +GRAB_AND_KILL\n    \
    ManualPickUpFilter = ANY +GRAB_AND_KILL\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n    AllowNeutralInside = Yes\n    AllowAlliesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object ZWounded\n  Side = Wild\n  KindOf = CLUB SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n    InitialHealth = 40\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZPrey\n  Side = Wild\n  KindOf = GRAB_AND_KILL INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ZKillSet\n  1 = Command_ZKill\nEnd\n\
CommandButton Command_ZKill\n  Command = SPECIAL_POWER\n  SpecialPower = ZKillPower\n  \
  Options = NEED_TARGET_ENEMY_OBJECT NEED_TARGET_NEUTRAL_OBJECT NEED_TARGET_ALLY_OBJECT\nEnd\n";

fn scene(giant: &str, target: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [giant.to_owned(), target.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let spawns = [
        (giant, Vec2::new(300.0, 300.0), PLAYER),
        (target, Vec2::new(314.0, 300.0), OTHER),
    ]
    .into_iter()
    .map(|(name, at, player)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                player,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_players(2)
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        );
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities[0], entities[1])
}

fn tree_scene(troll: &str, tree: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [troll.to_owned(), tree.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind(troll).unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap()];
    let forest_seeds = vec![crate::forest::ForestTreeInit {
        pos: Vec2::new(314.0, 300.0),
        kind: kinds.kind(tree).unwrap(),
        grabbable: true,
        harvestable: false,
        supply: 0,
        tainted: false,
        source_index: 0,
    }];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_players(2)
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_forest_seeds(forest_seeds);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let troll_entity = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec()[0];
    (simulation, ids, troll_entity)
}

fn select_and(simulation: &mut Simulation<RotwkRules>, giant: Entity, command: RotwkCommand) {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![giant],
        },
        command,
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
}

fn order_grab(simulation: &mut Simulation<RotwkRules>, giant: Entity, target: Entity) {
    select_and(
        simulation,
        giant,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(target),
            at: None,
        },
    );
}

fn run(simulation: &mut Simulation<RotwkRules>, frames: u32) {
    for _ in 0..frames {
        simulation.tick().unwrap();
    }
}

fn held_by(simulation: &Simulation<RotwkRules>, ids: Ids, target: Entity) -> Option<Entity> {
    simulation
        .state()
        .get(ids.contained, target)
        .map(|held| held.container)
}

#[test]
fn an_ordered_pick_up_takes_its_target_at_the_trigger_and_not_before() {
    let (mut simulation, ids, giant, log) = scene("ZGiant", "ZLog");
    order_grab(&mut simulation, giant, log);
    run(&mut simulation, 15);
    assert_eq!(
        held_by(&simulation, ids, log),
        None,
        "taken before the trigger"
    );
    run(&mut simulation, 60);
    assert_eq!(held_by(&simulation, ids, log), Some(giant));
}

#[test]
fn an_order_the_manual_pick_up_filter_refuses_takes_nothing() {
    let (mut simulation, ids, giant, grunt) = scene("ZGiant", "ZGrunt");
    order_grab(&mut simulation, giant, grunt);
    run(&mut simulation, 60);
    assert_eq!(held_by(&simulation, ids, grunt), None);
}

fn run_until(
    simulation: &mut Simulation<RotwkRules>,
    limit: u32,
    done: impl Fn(&Simulation<RotwkRules>) -> bool,
) -> u32 {
    for frame in 0..limit {
        if done(simulation) {
            return frame;
        }
        simulation.tick().unwrap();
    }
    panic!("not done within {limit} frames");
}

#[test]
fn an_eat_starts_when_the_grab_finishes_and_heals_at_once() {
    let (mut simulation, ids, giant, grunt) = scene("ZHungryGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::Debug(DebugCommand::Damage { percent: 50 }),
    );
    run_until(&mut simulation, 120, |simulation| {
        held_by(simulation, ids, grunt) == Some(giant)
    });
    let health = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .state()
            .get(ids.health, entity)
            .map(|h| h.0.current())
    };
    assert_eq!(
        health(&simulation, giant),
        Some(500.0),
        "healed at the grab"
    );
    run_until(&mut simulation, 120, |simulation| {
        !simulation.state().contains(ids.using_ability, giant)
    });
    assert_eq!(
        health(&simulation, giant),
        Some(600.0),
        "not healed at the finish"
    );
    assert_eq!(
        health(&simulation, grunt),
        Some(100.0),
        "eaten at the finish"
    );
    run(&mut simulation, 1);
    assert!(has(&simulation, giant, "EATING"));
    run(&mut simulation, 38);
    assert_eq!(health(&simulation, grunt), Some(100.0), "eaten too soon");
    run(&mut simulation, 6);
    assert!(health(&simulation, grunt).is_none_or(|left| left <= 0.0));
    assert_eq!(health(&simulation, giant), Some(600.0));
}

#[test]
fn a_grabbed_non_shrubbery_packs_the_frame_after_its_trigger() {
    let (mut simulation, ids, giant, log) = scene("ZGiant", "ZLog");
    order_grab(&mut simulation, giant, log);
    run_until(&mut simulation, 120, |simulation| {
        held_by(simulation, ids, log) == Some(giant)
    });
    assert!(!has(&simulation, giant, "PACKING"));
    run(&mut simulation, 1);
    assert!(has(&simulation, giant, "PACKING"));
}

fn held_count(simulation: &Simulation<RotwkRules>, ids: Ids, holder: Entity) -> usize {
    simulation
        .state()
        .store(ids.contained)
        .map(|store| {
            store
                .entities()
                .iter()
                .filter(|&&entity| held_by(simulation, ids, entity) == Some(holder))
                .count()
        })
        .unwrap_or(0)
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<f32> {
    simulation
        .state()
        .get(ids.health, entity)
        .map(|h| h.0.current())
}

#[test]
fn a_rock_vendor_hands_its_grab_object_to_the_grabber() {
    let (mut simulation, ids, giant, vendor) = scene("ZGiant", "ZRockVendor");
    order_grab(&mut simulation, giant, vendor);
    run_until(&mut simulation, 120, |simulation| {
        held_count(simulation, ids, giant) > 0
    });
    assert_eq!(held_by(&simulation, ids, vendor), None);
    assert_eq!(held_count(&simulation, ids, giant), 1);
    assert_eq!(health_of(&simulation, ids, vendor), Some(980.0));
}

#[test]
fn a_chunk_grab_vends_once_and_persists_for_its_count() {
    let (mut simulation, ids, giant, vendor) = scene("ZChunkGiant", "ZChunkVendor");
    order_grab(&mut simulation, giant, vendor);
    run_until(&mut simulation, 120, |simulation| {
        held_count(simulation, ids, giant) > 0
    });
    run(&mut simulation, 5);
    assert!(!has(&simulation, giant, "PACKING"));
    run(&mut simulation, 10);
    assert!(has(&simulation, giant, "PACKING"));
    assert_eq!(held_count(&simulation, ids, giant), 1);
    assert_eq!(health_of(&simulation, ids, vendor), Some(900.0));
}

#[test]
fn wake_auto_pick_up_wakes_a_matching_run_from_button_module_and_a_scan_follows() {
    let (mut simulation, ids, giant, grunt) = scene("ZButtonGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::WakeAutoPickUp { option: 1 },
    );
    run(&mut simulation, 90);
    assert_eq!(held_by(&simulation, ids, grunt), Some(giant));
}

#[test]
fn a_run_from_button_module_with_no_wake_never_scans() {
    let (mut simulation, ids, giant, grunt) = scene("ZButtonGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::SetStance { stance: 0 },
    );
    run(&mut simulation, 60);
    assert_eq!(held_by(&simulation, ids, grunt), None);
}

#[test]
fn wake_auto_pick_up_on_an_object_with_no_module_refuses() {
    let (mut simulation, ids, giant, grunt) = scene("ZGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::WakeAutoPickUp { option: 1 },
    );
    run(&mut simulation, 90);
    assert_eq!(held_by(&simulation, ids, grunt), None);
}

#[test]
fn an_auto_pick_up_filter_admitting_trees_converts_and_grabs_the_closest_one() {
    let (mut simulation, ids, troll) = tree_scene("ZTrollGiant", "ZOptTree");
    let forest_entity = crate::forest::singleton_of(simulation.state(), ids.forest).unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.forest, forest_entity)
            .unwrap()
            .len(),
        1,
        "the tree is not seeded"
    );
    run(&mut simulation, 90);
    assert_eq!(
        simulation
            .state()
            .get(ids.forest, forest_entity)
            .unwrap()
            .len(),
        0,
        "the tree is still standing"
    );
    let held = simulation
        .state()
        .store(ids.contained)
        .unwrap()
        .entities()
        .iter()
        .any(|&entity| {
            simulation
                .state()
                .get(ids.contained, entity)
                .is_some_and(|held| held.container == troll)
        });
    assert!(held, "the troll holds nothing");
}

#[test]
fn a_button_woken_tree_grab_takes_the_tree_it_converted_on_its_one_scan() {
    let (mut simulation, ids, troll) = tree_scene("ZButtonTroll", "ZOptTree");
    select_and(
        &mut simulation,
        troll,
        RotwkCommand::WakeAutoPickUp { option: 2 },
    );
    run(&mut simulation, 120);
    let held = simulation
        .state()
        .store(ids.contained)
        .unwrap()
        .entities()
        .iter()
        .any(|&entity| {
            simulation
                .state()
                .get(ids.contained, entity)
                .is_some_and(|held| held.container == troll)
        });
    assert!(held, "the troll holds nothing");
}

fn conditions(simulation: &Simulation<RotwkRules>, entity: Entity) -> Vec<String> {
    let state = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state, entity)
        .map(|held| {
            held.model_conditions
                .iter()
                .map(|name| name.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn has(simulation: &Simulation<RotwkRules>, entity: Entity, name: &str) -> bool {
    conditions(simulation, entity)
        .iter()
        .any(|held| held == name)
}

#[test]
fn a_grabbed_shrubbery_becomes_a_club_at_the_second_trigger_then_the_cast_packs() {
    let (mut simulation, ids, giant, tree) = scene("ZGiant", "ZOptTree");
    order_grab(&mut simulation, giant, tree);
    run(&mut simulation, 40);
    assert_eq!(held_by(&simulation, ids, tree), Some(giant));
    assert!(!has(&simulation, tree, "CLUB"));
    assert!(!has(&simulation, giant, "PACKING"));
    run(&mut simulation, 4);
    assert!(has(&simulation, tree, "CLUB"));
    run(&mut simulation, 1);
    assert!(has(&simulation, giant, "PACKING"));
}

#[test]
fn a_grabbed_non_shrubbery_gets_no_second_trigger() {
    let (mut simulation, ids, giant, log) = scene("ZGiant", "ZLog");
    order_grab(&mut simulation, giant, log);
    run(&mut simulation, 60);
    assert_eq!(held_by(&simulation, ids, log), Some(giant));
    assert!(!has(&simulation, log, "CLUB"));
}

#[test]
fn an_eat_marks_the_passenger_holds_the_eater_and_destroys_what_it_eats() {
    let (mut simulation, ids, giant, grunt) = scene("ZHungryGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::Debug(DebugCommand::Damage { percent: 50 }),
    );
    run_until(&mut simulation, 120, |simulation| {
        held_by(simulation, ids, grunt) == Some(giant)
    });
    run_until(&mut simulation, 120, |simulation| {
        !simulation.state().contains(ids.using_ability, giant)
    });
    run(&mut simulation, 1);
    assert!(
        has(&simulation, grunt, "EATING"),
        "the passenger's AnimState"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    assert!(
        simulation.state().contains(ids.deferred_order, giant),
        "the order waits out the busy hold"
    );
    let eaten = run_until(&mut simulation, 120, |simulation| {
        !simulation.state().is_live(grunt) || simulation.state().contains(ids.doomed, grunt)
    });
    assert!(eaten > 0);
    assert!(
        !simulation.state().is_live(grunt),
        "destroyed, not left as a corpse"
    );
    assert!(simulation.state().contains(ids.deferred_order, giant));
    run_until(&mut simulation, 120, |simulation| {
        !simulation.state().contains(ids.deferred_order, giant)
    });
    assert!(
        !has(&simulation, giant, "EATING"),
        "the hold ends with AnimTime"
    );
}

#[test]
fn a_grab_and_kill_target_is_marked_then_let_go_at_the_second_trigger() {
    let (mut simulation, ids, giant, prey) = scene("ZKillGiant", "ZPrey");
    order_grab(&mut simulation, giant, prey);
    run_until(&mut simulation, 120, |simulation| {
        held_by(simulation, ids, prey) == Some(giant)
    });
    run(&mut simulation, 1);
    assert!(has(&simulation, prey, "PREPARING"));
    run_until(&mut simulation, 60, |simulation| {
        held_by(simulation, ids, prey).is_none()
    });
    run(&mut simulation, 1);
    assert!(!has(&simulation, prey, "PREPARING"));
    assert!(has(&simulation, giant, "PACKING_TYPE_2"));
}

#[test]
fn a_grab_aimed_at_a_point_adopts_the_map_tree_there() {
    let (mut simulation, ids, troll) = tree_scene("ZGiant", "ZOptTree");
    select_and(
        &mut simulation,
        troll,
        RotwkCommand::FireSpecialPower {
            button: 1,
            target: None,
            at: Some((314.0, 300.0)),
        },
    );
    run_until(&mut simulation, 200, |simulation| {
        held_count(simulation, ids, troll) == 1
    });
    let tree = simulation
        .state()
        .store(ids.contained)
        .unwrap()
        .entities()
        .to_vec()[0];
    run_until(&mut simulation, 60, |simulation| {
        has(simulation, tree, "CLUB")
    });
}

#[test]
fn a_body_starts_at_its_initial_health() {
    let (simulation, ids, _, wounded) = scene("ZGiant", "ZWounded");
    let health = simulation.state().get(ids.health, wounded).unwrap().0;
    assert_eq!((health.current(), health.max()), (40.0, 100.0));
}

#[test]
fn a_paralyze_holds_the_casters_orders_after_its_cast() {
    let (mut simulation, ids, giant, vendor) = scene("ZChunkGiant", "ZChunkVendor");
    order_grab(&mut simulation, giant, vendor);
    run_until(&mut simulation, 200, |simulation| {
        held_count(simulation, ids, giant) > 0
            && !simulation.state().contains(ids.using_ability, giant)
    });
    run(&mut simulation, 1);
    assert!(simulation.state().contains(ids.ai_busy, giant));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 1);
    assert!(simulation.state().contains(ids.deferred_order, giant));
    let waited = run_until(&mut simulation, 60, |simulation| {
        !simulation.state().contains(ids.deferred_order, giant)
    });
    assert!(waited > 20, "held {waited} frames");
}

#[test]
fn a_grab_button_clicked_on_a_map_tree_takes_the_tree() {
    let (mut simulation, ids, troll) = tree_scene("ZGiant", "ZOptTree");
    select_and(
        &mut simulation,
        troll,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((314.0, 300.0)),
        },
    );
    run_until(&mut simulation, 200, |simulation| {
        held_count(simulation, ids, troll) == 1
    });
}
