use super::*;
use crate::rules::debug::{DebugCommand, DebugTarget};
use mint_sage_map::{StandingWaterArea, Water, WaterAreas};
use mint_sage_passability::WaterDepths;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Locomotor Hull\n  Surfaces = WATER DEEP_WATER\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Wide\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Barge\n  Side = Men\n  KindOf = SHIP SELECTABLE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Hull\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pikeman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Cohort\n  Side = Men\n  KindOf = SELECTABLE HORDE LARGE_RECTANGLE_PATHFIND\n  \
  CommandSet = HordeSet\n  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeContain Tag_01\n    Slots = 10\n    \
    RankInfo = RankNumber:1 UnitType:Pikeman Position:X:10 Y:-20 Position:X:10 Y:-10 Position:X:10 Y:0 Position:X:10 Y:10 Position:X:10 Y:20\n  \
  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 20\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = HordeSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet HordeSet\n  1 = Build_Cohort\nEnd\n\
CommandButton Build_Cohort\n  Command = UNIT_BUILD\n  Object = Cohort\nEnd\n";

fn lake(width: u32, height: u32, bed: u16, surface: u32) -> (HeightMapData, Water) {
    let map = HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![bed; (width * height) as usize],
    };
    #[allow(clippy::cast_precision_loss)]
    let (right, top) = (
        width as f32 * mint_sage_map::MAP_XY_FACTOR,
        height as f32 * mint_sage_map::MAP_XY_FACTOR,
    );
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
                points: vec![[0.0, 0.0], [right, 0.0], [right, top], [0.0, top]],
                water_height: surface,
                fx_shader: String::new(),
                depth_colors: String::new(),
            }],
        }),
        rivers: None,
        waves: None,
    };
    (map, water)
}

struct Scene {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    kinds: Arc<KindTable>,
    spawned: Vec<Entity>,
}

fn scene(spawns: &[(&str, Vec2)], flooded: bool) -> Scene {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Camp", "Cohort", "Wide", "Barge", "Pikeman"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let objects = spawns
        .iter()
        .map(|(name, at)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = if flooded {
        let (map, water) = lake(60, 60, 20, 100);
        Passability::builder(&map, 1.0)
            .water(&water, WaterDepths::SHIPPED)
            .build()
            .unwrap()
    } else {
        Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap()
    };
    let rules = RotwkRules::new(passability, objects)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation.tick().unwrap();
    let spawned = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    Scene {
        simulation,
        ids,
        kinds,
        spawned,
    }
}

impl Scene {
    fn adjust(&mut self, name: &str, at: Vec2) {
        let kind = self.kinds.kind(name).unwrap().0;
        self.simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::AdjustDestination {
                    target: DebugTarget::Kind { kind, side: None },
                    x: at.x,
                    y: at.y,
                }),
            )
            .unwrap();
        self.simulation.tick().unwrap();
    }
}

#[test]
fn a_non_centred_footprint_is_answered_half_a_unit_off_its_corner() {
    let mut scene = scene(&[("Wide", Vec2::new(100.0, 100.0))], false);
    scene.adjust("Wide", Vec2::new(500.0, 150.0));
    let at = position(&scene.simulation, scene.ids, scene.spawned[0]);
    assert!((at.x.rem_euclid(10.0) - 0.5).abs() < 0.01, "{at:?}");
    assert!((at.y.rem_euclid(10.0) - 0.5).abs() < 0.01, "{at:?}");
}

#[test]
fn a_ship_over_deep_water_is_answered_at_the_surface_height() {
    let mut scene = scene(&[("Barge", Vec2::new(100.0, 100.0))], true);
    scene.adjust("Barge", Vec2::new(300.0, 100.0));
    let at = position(&scene.simulation, scene.ids, scene.spawned[0]);
    assert!(at.minus(Vec2::new(300.0, 100.0)).length() < 12.0, "{at:?}");
}

#[test]
fn a_horde_rectangle_scales_by_its_members_over_slots() {
    let mut scene = scene(&[("Pikeman", Vec2::new(475.0, 505.0))], false);
    let recipe = scene
        .simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Cohort")
        .unwrap()
        .index;
    scene
        .simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 505.0,
                facing: 0.0,
            },
        )
        .unwrap();
    for _ in 0..4 {
        scene.simulation.tick().unwrap();
    }
    let horde = scene
        .simulation
        .state()
        .store(scene.ids.horde)
        .unwrap()
        .entities()[0];
    scene.adjust("Cohort", Vec2::new(505.0, 505.0));
    let at = position(&scene.simulation, scene.ids, horde);
    assert!(at.minus(Vec2::new(505.0, 505.0)).length() < 11.0, "{at:?}");
}
