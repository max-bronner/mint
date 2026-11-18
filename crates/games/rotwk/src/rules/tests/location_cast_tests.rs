use super::*;
use mint_sage_map::{StandingWaterArea, Water, WaterAreas};
use mint_sage_passability::WaterDepths;

const DATA: &str = "Locomotor TestFeet
  Surfaces = GROUND
  Speed = 30
  Acceleration = 500
  Braking = 500
    TurnTime = 1000
End
\
SpecialPower PowerInView\n  Enum = SPECIAL_SPELL_BOOK_HEAL\n  RadiusCursorRadius = 40\n  ReloadTime = 0\nEnd\n\
SpecialPower PowerNearStone\n  Enum = SPECIAL_GENERAL_TARGETLESS\n  Flags = NEEDS_OBJECT_FILTER\n  \
  ObjectFilter = NONE +STRUCTURE\n  RadiusCursorRadius = 40\n  ReloadTime = 0\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  ShroudClearingRange = 100\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = SpecialPowerModule Tag_InView\n    SpecialPowerTemplate = PowerInView\n  End\n  \
  Behavior = SpecialPowerModule Tag_NearStone\n    SpecialPowerTemplate = PowerNearStone\n  End\n  \
  CommandSet = CasterSet\n  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Stone\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CasterSet\n  1 = Fire_InView\n  2 = Fire_NearStone\nEnd\n\
CommandButton Fire_InView\n  Command = SPECIAL_POWER\n  SpecialPower = PowerInView\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Fire_NearStone\n  Command = SPECIAL_POWER\n  SpecialPower = PowerNearStone\n  Options = NEED_TARGET_POS\nEnd\n";

const CASTER_AT: Vec2 = Vec2::new(300.0, 300.0);
const STONE_AT: Vec2 = Vec2::new(340.0, 300.0);
const LAKE: [[f32; 2]; 4] = [
    [200.0, 380.0],
    [400.0, 380.0],
    [400.0, 900.0],
    [200.0, 900.0],
];

fn lake_map() -> Passability {
    let map = flat_map(96, 96);
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
                points: LAKE.to_vec(),
                water_height: 10,
                fx_shader: String::new(),
                depth_colors: String::new(),
            }],
        }),
        rivers: None,
        waves: None,
    };
    Passability::builder(&map, 1.0)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap()
}

fn pocket_map() -> Passability {
    let map = flat_map(96, 96);
    let strip = |unique_id: u32, points: [[f32; 2]; 4]| StandingWaterArea {
        unique_id,
        name: String::new(),
        layer_name: String::new(),
        uv_scroll_speed: 0.0,
        additive_blend: false,
        bump_texture: String::new(),
        sky_texture: String::new(),
        points: points.to_vec(),
        water_height: 10,
        fx_shader: String::new(),
        depth_colors: String::new(),
    };
    let moat = vec![
        strip(
            1,
            [
                [230.0, 180.0],
                [370.0, 180.0],
                [370.0, 190.0],
                [230.0, 190.0],
            ],
        ),
        strip(
            2,
            [
                [230.0, 290.0],
                [370.0, 290.0],
                [370.0, 300.0],
                [230.0, 300.0],
            ],
        ),
        strip(
            3,
            [
                [230.0, 180.0],
                [240.0, 180.0],
                [240.0, 300.0],
                [230.0, 300.0],
            ],
        ),
        strip(
            4,
            [
                [360.0, 180.0],
                [370.0, 180.0],
                [370.0, 300.0],
                [360.0, 300.0],
            ],
        ),
    ];
    let water = Water {
        standing: Some(WaterAreas {
            version: 2,
            areas: moat,
        }),
        rivers: None,
        waves: None,
    };
    Passability::builder(&map, 1.0)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap()
}

fn scenario() -> Simulation<RotwkRules> {
    scenario_on(lake_map())
}

fn scenario_on(map: Passability) -> Simulation<RotwkRules> {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster", "Stone"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let spawn = |name: &str, at: Vec2| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    };
    let spawns = vec![spawn("Caster", CASTER_AT), spawn("Stone", STONE_AT)];
    let rules = RotwkRules::new(map, spawns)
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_shroud(true);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let caster = simulation.state().store(ids.kind).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
            },
        )
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    simulation
}

fn refusal(simulation: &Simulation<RotwkRules>, button: u8, at: Vec2) -> Option<&'static str> {
    simulation.rules().power_target_refusal(
        simulation.state(),
        PLAYER,
        crate::rules::PowerButton::Selection(button),
        &crate::defeat::ObjectFilter::default(),
        None,
        Some((at.x, at.y)),
    )
}

#[test]
fn a_power_that_needs_the_ground_in_view_refuses_the_shroud() {
    let simulation = scenario();
    assert_eq!(refusal(&simulation, 0, CASTER_AT), None);
    assert_eq!(
        refusal(&simulation, 0, Vec2::new(40.0, 40.0)),
        Some("the position is under the shroud")
    );
}

#[test]
fn a_power_needing_its_object_filter_wants_an_admitted_object_under_it() {
    let simulation = scenario();
    assert_eq!(refusal(&simulation, 1, STONE_AT), None);
    assert!(refusal(&simulation, 1, CASTER_AT.minus(Vec2::new(60.0, 0.0))).is_some());
    assert!(
        refusal(&simulation, 1, Vec2::new(40.0, 40.0)).is_some(),
        "nothing admitted far off, whatever the shroud"
    );
}

#[test]
fn a_power_without_water_ok_refuses_water_under_its_circle() {
    let simulation = scenario();
    let reason = Some("water under the cast");
    assert_eq!(refusal(&simulation, 0, Vec2::new(300.0, 390.0)), reason);
    assert_eq!(
        refusal(&simulation, 0, Vec2::new(300.0, 350.0)),
        reason,
        "dry under the point, wet under the circle's rim"
    );
    assert_eq!(refusal(&simulation, 0, Vec2::new(300.0, 330.0)), None);
}

#[test]
fn seen_ground_no_selected_unit_reaches_is_not_a_move() {
    let simulation = scenario();
    let reaches = |at: Vec2| {
        simulation
            .rules()
            .selection_reaches(simulation.state(), PLAYER, at)
    };
    assert!(reaches(Vec2::new(300.0, 340.0)), "dry ground in sight");
    assert!(!reaches(Vec2::new(300.0, 390.0)), "deep water in sight");
    assert!(
        reaches(Vec2::new(300.0, 600.0)),
        "deep water under the shroud"
    );
}

#[test]
fn seen_passable_ground_cut_off_by_water_is_not_a_move() {
    let simulation = scenario_on(pocket_map());
    let reaches = |at: Vec2| {
        simulation
            .rules()
            .selection_reaches(simulation.state(), PLAYER, at)
    };
    assert!(
        !reaches(Vec2::new(300.0, 240.0)),
        "the island is passable and seen, but moated off"
    );
}
