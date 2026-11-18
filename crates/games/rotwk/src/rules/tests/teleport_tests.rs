const DATA: &str = "\
SpecialPower Blink\n  ReloadTime = 30000\nEnd\n\
Weapon Shove\n  AttackRange = 999\n  DelayBetweenShots = 1\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = TeleportSpecialAbilityUpdate Tag_Blink\n    SpecialPowerTemplate = Blink\n    \
    MaxDistance = 200\n    DestinationWeaponName = Shove\n    PreparationTime = 1\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  CommandSet = CasterSet\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Blink\nEnd\n\
CommandButton Fire_Blink\n  Command = SPECIAL_POWER\n  SpecialPower = Blink\n  \
  Options = NEED_TARGET_POS\nEnd\n";

use super::*;

fn scene() -> (Simulation<RotwkRules>, Ids, Entity) {
    scene_on(Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap())
}

fn scene_on(passability: Passability) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Caster".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("Caster").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec()[0];
    (simulation, ids, caster)
}

#[test]
fn a_cast_within_max_distance_jumps_the_caster_to_the_target() {
    let (mut simulation, ids, caster) = scene();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![caster],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((450.0, 300.0)),
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    let mut landed = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        let at = simulation.state().get(ids.body, caster).unwrap().0.position;
        if at.minus(Vec2::new(450.0, 300.0)).length() < 20.0 {
            landed = true;
            break;
        }
    }
    assert!(
        landed,
        "expected the caster to land at (450, 300), found {:?}",
        simulation.state().get(ids.body, caster).unwrap().0.position
    );
}

#[test]
fn a_targeted_cast_with_no_start_ability_range_still_triggers() {
    let (mut simulation, ids, caster) = scene();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![caster],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((450.0, 300.0)),
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    let mut landed = false;
    for _ in 0..30 {
        simulation.tick().unwrap();
        let at = simulation.state().get(ids.body, caster).unwrap().0.position;
        if at.minus(Vec2::new(450.0, 300.0)).length() < 20.0 {
            landed = true;
            break;
        }
    }
    assert!(
        landed,
        "expected the caster to trigger within 5 retail frames and land at (450, 300), found {:?}",
        simulation.state().get(ids.body, caster).unwrap().0.position
    );
}

#[test]
fn a_target_within_2d_but_past_3d_max_distance_is_refused() {
    let mut map = flat_map(96, 96);
    for y in 25..35 {
        for x in 40..50 {
            let index = (y * map.width + x) as usize;
            map.samples[index] = 300;
        }
    }
    let passability = Passability::build(&map, 1.0).unwrap();
    let (mut simulation, ids, caster) = scene_on(passability);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![caster],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((450.0, 300.0)),
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let at = simulation.state().get(ids.body, caster).unwrap().0.position;
    assert!(
        at.minus(Vec2::new(300.0, 300.0)).length() < 1.0,
        "expected the refused cast to leave the caster at its spawn point, found {at:?}"
    );
}

#[test]
fn a_cast_past_max_distance_is_refused() {
    let (mut simulation, ids, caster) = scene();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![caster],
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((900.0, 300.0)),
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let at = simulation.state().get(ids.body, caster).unwrap().0.position;
    assert!(
        at.minus(Vec2::new(300.0, 300.0)).length() < 1.0,
        "expected the refused cast to leave the caster at its spawn point, found {at:?}"
    );
}

const CAH_DATA: &str = "\
SpecialPower StoreIt\n  ReloadTime = 0\nEnd\n\
SpecialPower TeleportIt\n  ReloadTime = 0\n  ObjectFilter = ANY ALLIES\nEnd\n\
SpecialPower ReplaceIt\n  ReloadTime = 0\nEnd\n\
Object CahCaster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = StoreObjectsSpecialPower Tag_Store\n    SpecialPowerTemplate = StoreIt\n    \
    Radius = 60\n    PreparationTime = 1\n    IgnoreFacingCheck = Yes\n  End\n  \
  Behavior = TeleportToCasterSpecialPower Tag_Tele\n    SpecialPowerTemplate = TeleportIt\n    \
    Radius = 150\n    MinDestinationRadius = 50\n    MaxDestinationRadius = 100\n    \
    TargetFX = FX_Blink\n    TriggerFX = FX_Recall\n    \
    PreparationTime = 1\n    IgnoreFacingCheck = Yes\n  End\n  \
  Behavior = ReplaceObjectUpdate Tag_Replace\n    SpecialPowerTemplate = ReplaceIt\n    \
    ReplaceRadius = 150\n    PreparationTime = 1\n    IgnoreFacingCheck = Yes\n    \
    ReplaceObject\n      TargetObjectFilter = NONE +INFANTRY ENEMIES\n      \
      ReplacementObjectName = CahAnimal\n    End\n  End\n  \
  CommandSet = CahCasterSet\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet CahCasterSet\n  1 = Fire_Store\n  2 = Fire_Teleport\n  3 = Fire_Replace\nEnd\n\
CommandButton Fire_Store\n  Command = SPECIAL_POWER\n  SpecialPower = StoreIt\n  \
  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Fire_Teleport\n  Command = SPECIAL_POWER\n  SpecialPower = TeleportIt\n  \
  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Fire_Replace\n  Command = SPECIAL_POWER\n  SpecialPower = ReplaceIt\n  \
  Options = NEED_TARGET_POS\nEnd\n\
Object CahAlly\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor CahFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object CahEnemy\n  Side = Other\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = CahFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object CahAnimal\n  Side = Neutral\n  KindOf = ANIMAL SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 20\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object CahCage\n  Side = Other\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 400\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 1\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n";

struct CahMatch {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn cah_load() -> CahMatch {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        CAH_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["CahCaster", "CahAlly", "CahEnemy", "CahAnimal", "CahCage"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    CahMatch {
        data,
        kinds: Arc::new(kinds),
    }
}

fn cah_simulation(m: &CahMatch, placed: &[(&str, f32, f32, PlayerId)]) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let cast_rules = power_names.cast_rules(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![Some(0), Some(1)])
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules);
    Simulation::new(5, rules).unwrap()
}

fn cah_of_kind(
    m: &CahMatch,
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    name: &str,
) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

fn select_and_submit_cah(
    simulation: &mut Simulation<RotwkRules>,
    unit: Entity,
    command: RotwkCommand,
) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
}

#[test]
fn store_then_teleport_moves_only_the_stored_ally_and_empties_the_list() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahAlly", 300.0, 340.0, PLAYER),
            ("CahAlly", 700.0, 300.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let allies = cah_of_kind(&m, &simulation, ids, "CahAlly");
    let (stored, bystander) = if allies[0].index() < allies[1].index() {
        (allies[0], allies[1])
    } else {
        (allies[1], allies[0])
    };
    let bystander_start = simulation
        .state()
        .get(ids.body, bystander)
        .unwrap()
        .0
        .position;

    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: Some((300.0, 340.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 1,
            target: None,
            at: Some((700.0, 300.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }

    let stored_pos = simulation.state().get(ids.body, stored).unwrap().0.position;
    assert!(
        stored_pos.minus(Vec2::new(700.0, 300.0)).length() <= 100.0,
        "the stored ally must land within the ring around the click, found {stored_pos:?}"
    );
    let bystander_pos = simulation
        .state()
        .get(ids.body, bystander)
        .unwrap()
        .0
        .position;
    assert!(
        bystander_pos.minus(bystander_start).length() < 1.0,
        "the never-stored ally must not move: started {bystander_start:?}, now {bystander_pos:?}"
    );
}

#[test]
fn teleport_to_caster_gather_ignores_an_enemy_in_the_circle() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahAlly", 320.0, 300.0, PLAYER),
            ("CahEnemy", 280.0, 300.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let ally = cah_of_kind(&m, &simulation, ids, "CahAlly")[0];
    let enemy = cah_of_kind(&m, &simulation, ids, "CahEnemy")[0];
    let ally_start = simulation.state().get(ids.body, ally).unwrap().0.position;
    let enemy_start = simulation.state().get(ids.body, enemy).unwrap().0.position;

    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 1,
            target: None,
            at: Some((300.0, 300.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }

    let ally_pos = simulation.state().get(ids.body, ally).unwrap().0.position;
    assert!(
        ally_pos.minus(ally_start).length() > 5.0,
        "the ally must actually move off its spawn point, found {ally_pos:?}"
    );
    assert!(
        ally_pos.minus(Vec2::new(300.0, 300.0)).length() <= 100.0,
        "the ally must land near the caster, found {ally_pos:?}"
    );
    let enemy_pos = simulation.state().get(ids.body, enemy).unwrap().0.position;
    assert!(
        enemy_pos.minus(enemy_start).length() < 1.0,
        "the enemy must not move: started {enemy_start:?}, now {enemy_pos:?}"
    );
}

#[test]
fn teleport_target_fx_plays_on_each_moved_object() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahAlly", 320.0, 300.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let ally = cah_of_kind(&m, &simulation, ids, "CahAlly")[0];
    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 1,
            target: None,
            at: Some((300.0, 300.0)),
        },
    );
    let mut plays = Vec::new();
    for _ in 0..60 {
        let frame = simulation.tick().unwrap().frame;
        for event in simulation.rules().fx_events(simulation.state(), frame) {
            let name = simulation.rules().fx_name(event.list).unwrap_or("?");
            plays.push((name.to_owned(), event.on_object, event.caller));
        }
    }
    let blink = ("FX_Blink".to_owned(), true, ally);
    assert_eq!(
        plays.iter().filter(|play| **play == blink).count(),
        2,
        "{plays:?}"
    );
    assert!(
        plays.contains(&("FX_Recall".to_owned(), false, caster)),
        "{plays:?}"
    );
}

#[test]
fn three_gathered_allies_land_120_degrees_apart_facing_out() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahAlly", 320.0, 300.0, PLAYER),
            ("CahAlly", 300.0, 320.0, PLAYER),
            ("CahAlly", 280.0, 300.0, PLAYER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let allies = cah_of_kind(&m, &simulation, ids, "CahAlly");
    assert_eq!(allies.len(), 3, "the fixture's premise");

    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 1,
            target: None,
            at: Some((300.0, 300.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }

    let dest = Vec2::new(300.0, 300.0);
    let positions: Vec<Vec2> = allies
        .iter()
        .map(|&entity| simulation.state().get(ids.body, entity).unwrap().0.position)
        .collect();
    let mut angles: Vec<f32> = positions
        .iter()
        .map(|&at| {
            assert!(
                (at.minus(dest).length() - 50.0).abs() < 1.0,
                "expected the first (minimum) ring at 50, found {at:?} of all {positions:?}"
            );
            mint_math::f32::atan2(at.minus(dest).y, at.minus(dest).x)
        })
        .collect();
    angles.sort_by(f32::total_cmp);
    for pair in angles.windows(2) {
        let gap = (pair[1] - pair[0]).rem_euclid(core::f32::consts::TAU);
        assert!(
            (gap - core::f32::consts::TAU / 3.0).abs() < 0.05,
            "expected the three landings 120 degrees apart, found gaps {angles:?}"
        );
    }
}

#[test]
fn an_enemy_in_range_becomes_a_neutral_owned_replacement() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahEnemy", 320.0, 300.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let enemy = cah_of_kind(&m, &simulation, ids, "CahEnemy")[0];

    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 2,
            target: None,
            at: Some((300.0, 300.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }

    assert!(
        !simulation.state().is_live(enemy),
        "the replaced enemy must be dead and removed"
    );
    let animals = cah_of_kind(&m, &simulation, ids, "CahAnimal");
    assert_eq!(animals.len(), 1, "expected exactly one replacement animal");
    let neutral = simulation.rules().neutral_player();
    assert_eq!(
        simulation.state().get(ids.owner, animals[0]).map(|o| o.0),
        neutral,
        "the replacement must be owned by the neutral player, not the caster's"
    );
}

#[test]
fn a_garrisoned_enemy_is_not_replaced() {
    let m = cah_load();
    let mut simulation = cah_simulation(
        &m,
        &[
            ("CahCaster", 300.0, 300.0, PLAYER),
            ("CahCage", 340.0, 300.0, OTHER),
            ("CahEnemy", 312.0, 300.0, OTHER),
        ],
    );
    let ids = simulation.rules().ids().unwrap();
    let caster = cah_of_kind(&m, &simulation, ids, "CahCaster")[0];
    let cage = cah_of_kind(&m, &simulation, ids, "CahCage")[0];
    let enemy = cah_of_kind(&m, &simulation, ids, "CahEnemy")[0];

    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![enemy],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::Enter { target: cage })
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().contains(ids.contained, enemy),
        "the fixture's premise: the enemy must actually be inside the cage"
    );

    select_and_submit_cah(
        &mut simulation,
        caster,
        RotwkCommand::FireSpecialPower {
            button: 2,
            target: None,
            at: Some((300.0, 300.0)),
        },
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }

    assert!(
        simulation.state().is_live(enemy),
        "the garrisoned enemy must not be replaced or killed"
    );
    assert!(
        cah_of_kind(&m, &simulation, ids, "CahAnimal").is_empty(),
        "no replacement should be made for a garrisoned victim"
    );
}
