use super::*;

const DATA: &str = "\
SpecialPower ZGrabPower\n  Enum = SPECIAL_GRAB_PASSENGER\n  ReloadTime = 0\nEnd\n\
Object ZGiant\n  Side = Wild\n  KindOf = SELECTABLE\n  CommandSet = ZGiantSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = GrabPassengerSpecialPower Tag_Grab\n    SpecialPowerTemplate = ZGrabPower\n    \
    UpdateModuleStartsAttack = Yes\n  End\n  \
  Behavior = SpecialAbilityUpdate Tag_GrabUpdate\n    SpecialPowerTemplate = ZGrabPower\n    \
    StartAbilityRange = 8.0\n    UnpackTime = 300\n    PreparationTime = 500\n    PersistentPrepTime = 300\n    \
    PackTime = 1000\n    \
    GrabPassengerAnimAndDuration = AnimState:EATING AnimTime:3000 TriggerTime:1400\n    \
    GrabPassengerHealGainPercent = 10.0\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +CLUB +ORC\n    ManualPickUpFilter = ANY +CLUB -ORC\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n    AllowNeutralInside = Yes\n    AllowAlliesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
ChildObject ZHungryGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_Hunger\n    ScanDelayTime = 1000\n    ScanDistance = 200\n    \
    EatObjectEntry = MyHealth:99% TargetHealth:100% Filter:NONE +ORC\n  End\nEnd\n\
ChildObject ZButtonGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_ButtonScan\n    ScanDistance = 200\n    PickUpFilter = NONE +ORC\n    \
    RunFromButton = Yes\n    RunFromButtonNumber = 1\n  End\nEnd\n\
ChildObject ZTrollGiant ZGiant\n  \
  Behavior = AutoPickUpUpdate Tag_TreeHunt\n    ScanDistance = 200\n    PickUpFilter = NONE +TREE\n  End\nEnd\n\
Object ZLog\n  Side = Wild\n  KindOf = CLUB SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZOptTree\n  Side = Wild\n  KindOf = CLUB TREE SHRUBBERY IMMOBILE SELECTABLE\n  IsGrabbable = Yes\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZGrunt\n  Side = Wild\n  KindOf = ORC INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet ZGiantSet\n  1 = Command_ZGrab\nEnd\n\
CommandButton Command_ZGrab\n  Command = SPECIAL_POWER\n  SpecialPower = ZGrabPower\n  \
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
        .with_players(2);
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

#[test]
fn an_eat_kills_the_passenger_and_heals_the_eater_by_its_percent() {
    let (mut simulation, ids, giant, grunt) = scene("ZHungryGiant", "ZGrunt");
    select_and(
        &mut simulation,
        giant,
        RotwkCommand::Debug(DebugCommand::Damage { percent: 50 }),
    );
    run(&mut simulation, 40);
    assert_eq!(held_by(&simulation, ids, grunt), Some(giant));
    let health = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .state()
            .get(ids.health, entity)
            .map(|h| h.0.current())
    };
    assert_eq!(
        health(&simulation, giant),
        Some(500.0),
        "healed before the eat"
    );
    run(&mut simulation, 40);
    assert!(
        !simulation.state().contains(ids.using_ability, giant),
        "the grab is still casting"
    );
    assert_eq!(health(&simulation, grunt), Some(100.0), "eaten too soon");
    run(&mut simulation, 80);
    assert!(health(&simulation, grunt).is_none_or(|left| left <= 0.0));
    assert_eq!(health(&simulation, giant), Some(600.0));
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
