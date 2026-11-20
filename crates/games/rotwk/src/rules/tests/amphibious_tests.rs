use super::*;
use mint_sage_map::{StandingWaterArea, Water, WaterAreas};
use mint_sage_passability::WaterDepths;

const DATA: &str = "\
Locomotor TestFeet\n  Surfaces = GROUND\n  Speed = 60\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\nEnd\n\
Locomotor TestHull\n  Surfaces = WATER DEEP_WATER\n  Speed = 40\n  Acceleration = 500\n  \
  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Ferry\n  Side = Men\n  KindOf = SHIP AMPHIBIOUS SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Hold\n    \
    ObjectStatusOfContained = UNSELECTABLE\n    Slots = 2\n    \
    PassengerFilter = NONE +Rider\n    AllowOwnPlayerInsideOverride = Yes\n    \
    InitialPayload = Rider 2\n    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    \
    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  LocomotorSet\n    Locomotor = TestHull\n    Condition = SET_NORMAL\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 40\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  LocomotorSet\n    Locomotor = TestFeet\n    Condition = SET_NORMAL\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn half_lake_map(width: u32, height: u32) -> (HeightMapData, Water) {
    let samples = vec![0u16; (width * height) as usize];
    let map = HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples,
    };
    #[allow(clippy::cast_precision_loss)]
    let split_x_world = (width / 2) as f32 * mint_sage_map::MAP_XY_FACTOR;
    #[allow(clippy::cast_precision_loss)]
    let height_world = height as f32 * mint_sage_map::MAP_XY_FACTOR;
    let water = Water {
        standing: Some(WaterAreas {
            version: 2,
            areas: vec![StandingWaterArea {
                unique_id: 1,
                name: String::new(),
                layer_name: String::new(),
                uv_scroll_speed: 0.0,
                additive_blend: false,
                bump_texture: String::new(),
                sky_texture: String::new(),
                points: vec![
                    [0.0, 0.0],
                    [split_x_world, 0.0],
                    [split_x_world, height_world],
                    [0.0, height_world],
                ],
                water_height: 10,
                fx_shader: String::new(),
                depth_colors: String::new(),
            }],
        }),
        rivers: None,
        waves: None,
    };
    (map, water)
}

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, crate::unit::Kind) {
    scene_of(DATA, 100.0)
}

fn scene_of(data: &str, ferry_x: f32) -> (Simulation<RotwkRules>, Ids, Entity, crate::unit::Kind) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Ferry".to_owned(), "Rider".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("Ferry").unwrap(),
            Vec2::new(ferry_x, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let (map, water) = half_lake_map(96, 60);
    let passability = Passability::builder(&map, 1.0)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(1);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let ferry = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec()[0];
    let rider_kind = kinds.kind("Rider").unwrap();
    (simulation, ids, ferry, rider_kind)
}

#[test]
fn a_ferry_ordered_onto_land_taxis_and_unloads_toward_the_click() {
    let (mut simulation, ids, ferry, rider_kind) = scene();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ferry],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut unloaded = false;
    for _ in 0..1_000 {
        simulation.tick().unwrap();
        let riders_outside = simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(entity, kind)| {
                simulation.state().is_live(*entity)
                    && **kind == rider_kind
                    && !simulation.state().contains(ids.contained, *entity)
            })
            .count();
        if riders_outside > 0 {
            unloaded = true;
            break;
        }
    }
    assert!(
        unloaded,
        "expected at least one Rider to have left the ferry"
    );
}

#[test]
fn a_ship_with_no_exit_paths_lands_its_passenger_on_the_shore() {
    let data = DATA.replace(
        "ExitOffset = X:30.0 Y:0.0 Z:0.0",
        "ExitOffset = X:-30.0 Y:0.0 Z:0.0
    NumberOfExitPaths = 0",
    );
    let (mut simulation, ids, ferry, rider_kind) = scene_of(&data, 460.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ferry],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let passenger = simulation
        .state()
        .store(ids.contained)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Exit {
                passenger: Some(passenger),
            },
        )
        .unwrap();
    let outside = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(entity, kind)| {
                **kind == rider_kind && !simulation.state().contains(ids.contained, *entity)
            })
            .map(|(entity, _)| entity)
            .next()
    };
    let mut loaded = false;
    for _ in 0..1_000 {
        simulation.tick().unwrap();
        loaded |= simulation
            .state()
            .store(ids.contained)
            .is_some_and(|s| !s.is_empty());
        if !loaded {
            continue;
        }
        if let Some(rider) = outside(&simulation) {
            let at = simulation.state().get(ids.body, rider).unwrap().0.position;
            assert!(at.x >= 480.0, "landed in the water at x = {}", at.x);
            return;
        }
    }
    panic!("no rider left the ferry");
}

#[test]
fn two_passengers_let_out_of_a_ship_fan_out_along_the_shore() {
    let data = DATA.replace(
        "ExitOffset = X:30.0 Y:0.0 Z:0.0",
        "ExitOffset = X:-30.0 Y:0.0 Z:0.0
    NumberOfExitPaths = 0",
    );
    let (mut simulation, ids, ferry, rider_kind) = scene_of(&data, 460.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![ferry],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let passengers: Vec<Entity> = simulation
        .state()
        .store(ids.contained)
        .unwrap()
        .entities()
        .to_vec();
    assert_eq!(passengers.len(), 2);
    for passenger in passengers {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Exit {
                    passenger: Some(passenger),
                },
            )
            .unwrap();
    }
    let mut goals: Vec<(Entity, Vec2)> = Vec::new();
    for _ in 0..1_000 {
        simulation.tick().unwrap();
        for (entity, kind) in simulation.state().store(ids.kind).unwrap().iter() {
            if *kind == rider_kind
                && !simulation.state().contains(ids.contained, entity)
                && !goals.iter().any(|(seen, _)| *seen == entity)
            {
                if let Some(path) = simulation.state().get(ids.path, entity) {
                    goals.push((entity, path.goal));
                }
            }
        }
        if goals.len() == 2 {
            break;
        }
    }
    assert_eq!(goals.len(), 2, "both riders should have left: {goals:?}");
    assert_ne!(goals[0].1, goals[1].1, "both sent to {:?}", goals[0].1);
}
