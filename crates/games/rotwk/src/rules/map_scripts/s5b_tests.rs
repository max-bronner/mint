use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::{
    Condition, Dictionary, HeightMapData, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
    ScriptTree, TeamDef,
};
use mint_sage_passability::Passability;
use mint_sim::{Entity, PlayerId, Simulation};

use super::team::TeamProtos;
use super::{MapScriptEngine, MapScriptState};
use crate::rules::guard::{GuardMode, GuardState, GuardTarget};
use crate::rules::{RotwkCommand, RotwkRules};
use crate::templates::KindTable;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Locomotor Wings\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
StanceTemplate PlainStances\n  Stance Battle\n  End\n  Stance HoldGround\n  End\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = StancesBehavior Tag_Stance\n    StanceTemplate = PlainStances\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Raven\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = Wings\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Spearman\nEnd\n\
CommandButton Build_Spearman\n  Command = UNIT_BUILD\n  Object = Spearman\nEnd\n\
Object Brute\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Sword\n  AttackRange = 20\n  MeleeWeapon = Yes\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Weapon Bow\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Swordbow\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n    Weapon = SECONDARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Bowsword\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n    Weapon = SECONDARY Sword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fragile\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Lance\n  AttackRange = 50\n  ContinueAttackRange = 200\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 4000\n    DamageType = PIERCE\n  End\nEnd\n\
Object Lancer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Lance\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Brawler\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const MINE: PlayerId = PlayerId::new(0);

fn text(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

fn number(value: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: value,
        real_value: 0.0,
        string_value: String::new(),
    }
}

fn real(value: f32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: value,
        string_value: String::new(),
    }
}

fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

fn script(
    name: &str,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
) -> Script {
    let (condition_name, params) = condition.unwrap_or(("CONDITION_TRUE", Vec::new()));
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot: true,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions: vec![OrCondition {
            conditions: vec![Condition {
                condition_type: 0,
                inverted: false,
                enabled: true,
                name: condition_name.to_owned(),
                params,
            }],
        }],
        actions,
        actions_false: Vec::new(),
        sequence: Default::default(),
    }
}

fn after(name: &str, unit: &str, actions: Vec<ScriptAction>) -> Script {
    script(name, Some(("NAMED_CREATED", vec![text(unit)])), actions)
}

fn make(unit: &str, template: &str, team: &str, at: &str) -> ScriptAction {
    act(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![text(unit), text(template), text(team), text(at)],
    )
}

fn team(name: &str) -> TeamDef {
    TeamDef {
        owner: String::new(),
        name: name.to_owned(),
        singleton: true,
        properties: Dictionary::from_pairs(Vec::new()),
    }
}

fn square(low: f32, high: f32) -> Vec<(f32, f32)> {
    vec![(low, low), (high, low), (high, high), (low, high)]
}

fn simulate(scripts: Vec<Script>, ticks: u32) -> Simulation<RotwkRules> {
    simulate_over(scripts, ticks, false)
}

fn simulate_over(scripts: Vec<Script>, ticks: u32, with_camp: bool) -> Simulation<RotwkRules> {
    simulate_sides(scripts, Vec::new(), ticks, with_camp)
}

fn simulate_sides(
    scripts: Vec<Script>,
    wild_scripts: Vec<Script>,
    ticks: u32,
    with_camp: bool,
) -> Simulation<RotwkRules> {
    simulate_on(scripts, wild_scripts, ticks, with_camp, false)
}

fn simulate_on(
    scripts: Vec<Script>,
    wild_scripts: Vec<Script>,
    ticks: u32,
    with_camp: bool,
    edge: bool,
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(DATA);
    let names = [
        "Spearman", "Raven", "Quarry", "Camp", "Brute", "Swordbow", "Bowsword", "Fragile",
        "Lancer", "Brawler",
    ]
    .map(String::from);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        ("Home", (Vec2::new(60.0, 60.0), 0.0)),
        ("InZone", (Vec2::new(300.0, 300.0), 0.0)),
        ("Far", (Vec2::new(400.0, 60.0), 0.0)),
        ("Near", (Vec2::new(100.0, 60.0), 0.0)),
        ("Mid", (Vec2::new(160.0, 60.0), 0.0)),
        ("Off", (Vec2::new(60.0, -40.0), 0.0)),
        ("Rim", (Vec2::new(60.0, -10.0), 0.0)),
        ("Rim2", (Vec2::new(60.0, -25.0), 0.0)),
        ("Rim3", (Vec2::new(60.0, -45.0), 0.0)),
        ("Lip", (Vec2::new(60.0, 5.0), 0.0)),
        ("Out", (Vec2::new(60.0, 170.0), 0.0)),
        ("WardA", (Vec2::new(500.0, 500.0), 0.0)),
        ("WardB", (Vec2::new(515.0, 515.0), 0.0)),
        ("WardC", (Vec2::new(521.0, 500.0), 0.0)),
        ("WardD", (Vec2::new(530.0, 500.0), 0.0)),
        ("WardLeft", (Vec2::new(400.0, 500.0), 0.0)),
        ("WardRight", (Vec2::new(600.0, 500.0), 0.0)),
        ("PostNear", (Vec2::new(500.0, 520.0), 0.0)),
        ("PostFar", (Vec2::new(500.0, 800.0), 0.0)),
        ("Bait", (Vec2::new(500.0, 396.0), 0.0)),
        ("Beside", (Vec2::new(500.0, 560.0), 0.0)),
    ]
    .into_iter()
    .map(|(name, at)| (name.to_owned(), at))
    .collect();
    let areas: BTreeMap<String, Vec<(f32, f32)>> = [
        (String::from("Zone"), square(250.0, 350.0)),
        (String::from("Empty"), square(500.0, 550.0)),
        (String::from("Everywhere"), square(0.0, 1000.0)),
        (
            String::from("Edge"),
            vec![(0.0, -80.0), (200.0, -80.0), (200.0, 100.0), (0.0, 100.0)],
        ),
    ]
    .into_iter()
    .collect();
    let tree = ScriptTree {
        player_slots: vec![
            scripts.into_iter().map(ScriptNode::Script).collect(),
            wild_scripts.into_iter().map(ScriptNode::Script).collect(),
        ],
    };
    let sides = vec![vec![String::new(), String::from("Wilds")]];
    let wild = TeamDef {
        owner: String::from("Wilds"),
        name: String::from("teamWilds"),
        singleton: true,
        properties: Dictionary::from_pairs(vec![(
            String::from("teamAttackCommonTarget"),
            mint_sage_map::Value::Bool(true),
        )]),
    };
    let protos = TeamProtos::from_defs(&[
        team("teamPlayer_1"),
        team("teamPlayer_2"),
        team("teamWard"),
        wild,
    ])
    .unwrap();
    let engine = MapScriptEngine::build(&[tree], &sides, &waypoints, &areas, false)
        .with_team_prototypes(protos);
    let map = HeightMapData {
        width: 160,
        height: 160,
        border_size: if edge { 10 } else { 0 },
        boundaries: if edge {
            vec![mint_sage_map::Boundary { x: 139, y: 139 }]
        } else {
            Vec::new()
        },
        samples: vec![0; 160 * 160],
    };
    let passability = if edge {
        Passability::builder(&map, 5.0 / 128.0)
            .playable_border(true)
            .build()
            .unwrap()
    } else {
        Passability::build(&map, 5.0 / 128.0).unwrap()
    };
    let camps: Vec<_> = with_camp
        .then(|| {
            let kind = kinds.kind("Camp").unwrap();
            let at = Vec2::new(60.0, 60.0);
            let mut camp = kinds
                .spawn(&data.combat, kind, at, Facing::EAST, MINE)
                .unwrap();
            camp.producer = kinds.producer(&production, kind, at, Facing::EAST);
            camp
        })
        .into_iter()
        .collect();
    let rules = RotwkRules::new(passability, camps)
        .with_script_object_names(if with_camp {
            vec![(0, String::from("Camp"))]
        } else {
            Vec::new()
        })
        .with_players(2)
        .with_teams(vec![None, None])
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations {
                enemies: vec![MINE],
                ..crate::alliance::MapRelations::default()
            }],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_stances(Arc::clone(&data.stances))
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    tick(&mut simulation, ticks);
    simulation
}

fn tick(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn state_of(simulation: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

fn named(simulation: &Simulation<RotwkRules>, name: &str) -> Entity {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(entity, tag)| tag.0 == name && simulation.state().is_live(*entity))
                .map(|(entity, _)| entity)
        })
        .unwrap_or_else(|| panic!("{name} was not created"))
}

fn owner_of(simulation: &Simulation<RotwkRules>, unit: &str) -> PlayerId {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .get(ids.owner, named(simulation, unit))
        .unwrap()
        .0
}

fn position_of(simulation: &Simulation<RotwkRules>, unit: &str) -> Vec2 {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .get(ids.body, named(simulation, unit))
        .unwrap()
        .0
        .position
}

fn purse(simulation: &Simulation<RotwkRules>, player: PlayerId) -> i64 {
    let ids = simulation.rules().ids().unwrap();
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn stand_ground_run(stand: bool) -> (Option<mint_sage_ini::Stance>, bool) {
    let mut scripts = vec![script(
        "Make",
        None,
        vec![make("Walker", "Spearman", "teamPlayer_1", "Home")],
    )];
    if stand {
        scripts.push(after(
            "Hold",
            "Walker",
            vec![act("UNIT_STAND_GROUND", vec![text("Walker"), number(1)])],
        ));
    }
    let sim = simulate(scripts, 60);
    let ids = sim.rules().ids().unwrap();
    let walker = named(&sim, "Walker");
    (
        sim.state().get(ids.stance, walker).map(|held| held.0),
        sim.rules().stands_ground(sim.state(), ids, walker),
    )
}

#[test]
fn stand_ground_sets_the_status_and_leaves_the_stance_alone() {
    let (stance, stands) = stand_ground_run(false);
    assert!(!stands);
    let (held, stands) = stand_ground_run(true);
    assert!(stands);
    assert_eq!(held, stance);
}

fn walked_after_ordered_at_far_target(stand: bool) -> f32 {
    let mut orders = vec![act(
        "TEAM_ATTACK_NAMED",
        vec![text("teamPlayer_1"), text("Target")],
    )];
    if stand {
        orders.insert(0, act("UNIT_STAND_GROUND", vec![text("Walker"), number(1)]));
    }
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Target", "Quarry", "teamPlayer_2", "Far"),
                ],
            ),
            after("Order", "Target", orders),
        ],
        150,
    );
    let (at, home) = (position_of(&sim, "Walker"), Vec2::new(60.0, 60.0));
    at.minus(home).length()
}

#[test]
fn a_stand_ground_unit_does_not_walk_to_a_target_out_of_reach() {
    let free = walked_after_ordered_at_far_target(false);
    let held = walked_after_ordered_at_far_target(true);
    assert!(free > 30.0, "the control walked {free}");
    assert!(held < 1.0, "the stand-ground unit walked {held}");
}

fn attack_area_run(area: &str) -> (Simulation<RotwkRules>, Entity) {
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Target", "Quarry", "teamPlayer_2", "InZone"),
                ],
            ),
            after(
                "Attack",
                "Target",
                vec![act("NAMED_ATTACK_AREA", vec![text("Walker"), text(area)])],
            ),
        ],
        120,
    );
    let walker = named(&sim, "Walker");
    (sim, walker)
}

#[test]
fn an_attack_area_unit_goes_for_the_enemy_inside_the_area() {
    let (sim, walker) = attack_area_run("Zone");
    let ids = sim.rules().ids().unwrap();
    let target = named(&sim, "Target");
    let order = sim.state().get(ids.attack_order, walker).copied();
    assert_eq!(order.map(|order| order.target), Some(target), "{order:?}");
}

#[test]
fn an_attack_area_order_ends_when_a_scan_finds_nobody() {
    let (sim, walker) = attack_area_run("Empty");
    let ids = sim.rules().ids().unwrap();
    assert!(!state_of(&sim).rt.attack_areas.holds(walker));
    assert!(!sim.state().contains(ids.attack_order, walker));
}

#[test]
fn an_area_guard_from_a_waypoint_engages_what_stands_in_the_area_beyond_the_waypoints_reach() {
    let mut sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Watcher", "Raven", "teamPlayer_1", "InZone"),
                    make("Target", "Quarry", "teamPlayer_2", "InZone"),
                ],
            ),
            after(
                "Guard",
                "Target",
                vec![act(
                    "UNIT_GUARD_AREA_FROM_POSITION",
                    vec![text("Walker"), text("Zone"), text("Home")],
                )],
            ),
        ],
        30,
    );
    let ids = sim.rules().ids().unwrap();
    let (walker, target) = (named(&sim, "Walker"), named(&sim, "Target"));
    let mut engaged = false;
    for _ in 0..400 {
        tick(&mut sim, 1);
        engaged |= sim
            .state()
            .get(ids.guard, walker)
            .is_some_and(|guard| guard.nemesis == Some(target));
    }
    assert!(engaged);
}

#[test]
fn a_hunter_rescans_every_fifteen_script_frames() {
    let mut sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Target", "Quarry", "teamPlayer_2", "Far"),
                ],
            ),
            after(
                "Hunt",
                "Target",
                vec![act("TEAM_HUNT", vec![text("teamPlayer_1")])],
            ),
        ],
        20,
    );
    let walker = named(&sim, "Walker");
    let mut due: Vec<u64> = Vec::new();
    for _ in 0..600 {
        tick(&mut sim, 1);
        if let Some(at) = state_of(&sim).rt.hunters.next_scan(walker) {
            if due.last() != Some(&at) {
                due.push(at);
            }
        }
    }
    assert!(due.len() >= 4, "{due:?}");
    assert!(
        due.windows(2).all(|pair| pair[1] - pair[0] == 15),
        "{due:?}"
    );
}

fn surface_hit(template: &str, surface: i32) -> i32 {
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![make("Walker", template, "teamPlayer_1", "InZone")],
            ),
            script(
                "Inside",
                Some((
                    "TEAM_INSIDE_AREA_PARTIALLY",
                    vec![text("teamPlayer_1"), text("Zone"), number(surface)],
                )),
                vec![act("INCREMENT_COUNTER", vec![number(1), text("hit")])],
            ),
        ],
        60,
    );
    state_of(&sim).counter_value("", "hit")
}

#[test]
fn a_team_area_condition_counts_only_members_whose_locomotor_crosses_the_surface() {
    assert_eq!(surface_hit("Spearman", 0), 1, "ground unit, GROUND");
    assert_eq!(surface_hit("Spearman", 3), 0, "ground unit, AIR");
    assert_eq!(surface_hit("Raven", 3), 1, "flier, AIR");
    assert_eq!(surface_hit("Camp", 0), 1, "no locomotor counts for GROUND");
    assert_eq!(
        surface_hit("Camp", 3),
        0,
        "no locomotor never counts for AIR"
    );
}

fn camp_scenario(extra: Vec<Script>) -> Simulation<RotwkRules> {
    let mut sim = simulate_over(extra, 20, true);
    let camp = named(&sim, "Camp");
    sim.submit(
        MINE,
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![camp],
        },
    )
    .unwrap();
    tick(&mut sim, 1);
    sim.submit(MINE, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    sim
}

#[test]
fn a_named_transfer_leaves_the_queue_unrefunded() {
    let mut sim = camp_scenario(vec![script(
        "Hand over",
        Some((
            "PLAYER_HAS_CREDITS",
            vec![number(99_950), number(4), text("Player_1")],
        )),
        vec![act(
            "NAMED_TRANSFER_OWNERSHIP_PLAYER",
            vec![text("Camp"), text("Player_2")],
        )],
    )]);
    tick(&mut sim, 40);
    assert_eq!(owner_of(&sim, "Camp"), PlayerId::new(1));
    assert_eq!(purse(&sim, MINE), 99_900);
}

#[test]
fn a_produced_unit_is_worth_the_cost_paid_for_it() {
    let mut sim = camp_scenario(vec![script(
        "Worth",
        Some((
            "SKIRMISH_VALUE_IN_AREA",
            vec![text("Player_1"), number(3), real(70.0), text("Everywhere")],
        )),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("worth")])],
    )]);
    tick(&mut sim, 300);
    assert_eq!(state_of(&sim).counter_value("", "worth"), 1);
}

#[test]
fn an_idle_member_takes_the_target_its_team_attacks() {
    let sim = simulate_sides(
        vec![script(
            "MakeTarget",
            None,
            vec![make("Target", "Quarry", "teamPlayer_1", "Near")],
        )],
        vec![script(
            "Make",
            None,
            vec![
                make("Striker", "Brute", "teamWilds", "Home"),
                make("Idler", "Brute", "teamWilds", "Far"),
            ],
        )],
        200,
        false,
    );
    let ids = sim.rules().ids().unwrap();
    let target = named(&sim, "Target");
    let target_of = |unit: &str| {
        sim.state()
            .get(ids.armed, named(&sim, unit))
            .and_then(|armed| armed.target)
    };
    assert_eq!(target_of("Striker"), Some(target));
    assert_eq!(target_of("Idler"), Some(target));
}

fn swordbow_walked(stand: bool) -> f32 {
    let mut hold = vec![act("UNIT_STAND_GROUND", vec![text("Walker"), number(1)])];
    if !stand {
        hold.clear();
    }
    hold.push(make("Target", "Quarry", "teamPlayer_2", "Mid"));
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![make("Walker", "Swordbow", "teamPlayer_1", "Home")],
            ),
            after("Hold", "Walker", hold),
        ],
        240,
    );
    let at = position_of(&sim, "Walker");
    let (dx, dy) = (at.x - 60.0, at.y - 60.0);
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn a_stand_ground_units_melee_primary_out_of_reach_fails_the_slot_test() {
    let free = swordbow_walked(false);
    let held = swordbow_walked(true);
    assert!(free > 30.0, "the control walked {free}");
    assert!(held < 1.0, "the stand-ground unit walked {held}");
}

#[test]
fn a_team_transfer_hands_later_members_to_the_new_player() {
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![make("First", "Quarry", "teamPlayer_1", "Home")],
            ),
            after(
                "Give",
                "First",
                vec![
                    act(
                        "TEAM_TRANSFER_TO_PLAYER",
                        vec![text("teamPlayer_1"), text("Player_2")],
                    ),
                    make("Later", "Quarry", "teamPlayer_1", "Near"),
                ],
            ),
        ],
        60,
    );
    let first = owner_of(&sim, "First");
    assert_ne!(first, MINE, "the member moved");
    assert_eq!(owner_of(&sim, "Later"), first);
}

#[test]
fn a_stand_ground_unit_switched_to_an_out_of_reach_melee_slot_does_not_walk() {
    let mut hold = vec![act("UNIT_STAND_GROUND", vec![text("Walker"), number(1)])];
    hold.push(make("Target", "Quarry", "teamPlayer_2", "Mid"));
    let sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![make("Walker", "Bowsword", "teamPlayer_1", "Home")],
            ),
            after("Hold", "Walker", hold),
        ],
        240,
    );
    let at = position_of(&sim, "Walker");
    let (dx, dy) = (at.x - 60.0, at.y - 60.0);
    let walked = (dx * dx + dy * dy).sqrt();
    assert!(walked < 1.0, "the stand-ground unit walked {walked}");
}

fn edge_run(scripts: Vec<Script>, ticks: u32) -> Simulation<RotwkRules> {
    simulate_on(scripts, Vec::new(), ticks, false, true)
}

fn edge_wilds_run(scripts: Vec<Script>, ticks: u32) -> Simulation<RotwkRules> {
    simulate_on(Vec::new(), scripts, ticks, false, true)
}

fn attack_target(sim: &Simulation<RotwkRules>, unit: Entity) -> Option<Entity> {
    let ids = sim.rules().ids().unwrap();
    sim.state().get(ids.attack_order, unit).map(|o| o.target)
}

#[test]
fn a_hunter_leaves_a_foe_standing_beyond_the_map_edge() {
    assert!(hunted("Mid"), "the on-map control was not hunted");
    assert!(!hunted("Off"));
}

fn hunted(target_at: &str) -> bool {
    let mut sim = edge_run(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Target", "Quarry", "teamPlayer_2", target_at),
                ],
            ),
            after(
                "Hunt",
                "Target",
                vec![act("TEAM_HUNT", vec![text("teamPlayer_1")])],
            ),
        ],
        20,
    );
    let (walker, target) = (named(&sim, "Walker"), named(&sim, "Target"));
    let mut engaged = false;
    for _ in 0..200 {
        tick(&mut sim, 1);
        engaged |= attack_target(&sim, walker) == Some(target);
    }
    engaged
}

#[test]
fn an_attack_area_unit_leaves_a_foe_in_the_unplayable_ring() {
    assert!(area_attacked("Near"), "the on-map control was not attacked");
    assert!(!area_attacked("Off"));
}

fn area_attacked(target_at: &str) -> bool {
    let mut sim = edge_run(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamPlayer_1", "Home"),
                    make("Target", "Quarry", "teamPlayer_2", target_at),
                ],
            ),
            after(
                "Attack",
                "Target",
                vec![act("NAMED_ATTACK_AREA", vec![text("Walker"), text("Edge")])],
            ),
        ],
        20,
    );
    let (walker, target) = (named(&sim, "Walker"), named(&sim, "Target"));
    let mut engaged = false;
    for _ in 0..200 {
        tick(&mut sim, 1);
        engaged |= attack_target(&sim, walker) == Some(target);
    }
    engaged
}

#[test]
fn a_guard_leaves_a_foe_standing_beyond_the_map_edge() {
    assert!(guarded("Near"), "the on-map control was not taken");
    assert!(!guarded("Off"));
}

fn guarded(target_at: &str) -> bool {
    let mut sim = edge_wilds_run(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Spearman", "teamWilds", "Home"),
                    make("Target", "Quarry", "teamPlayer_1", target_at),
                ],
            ),
            after(
                "Guard",
                "Target",
                vec![act(
                    "UNIT_GUARD_AREA_FROM_POSITION",
                    vec![text("Walker"), text("Edge"), text("Home")],
                )],
            ),
        ],
        30,
    );
    let ids = sim.rules().ids().unwrap();
    let (walker, target) = (named(&sim, "Walker"), named(&sim, "Target"));
    let mut engaged = false;
    for _ in 0..400 {
        tick(&mut sim, 1);
        engaged |= sim
            .state()
            .get(ids.guard, walker)
            .is_some_and(|guard| guard.nemesis == Some(target));
    }
    engaged
}

#[test]
fn a_continued_attack_skips_a_foe_across_the_map_edge() {
    let mut sim = edge_run(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Lancer", "teamPlayer_1", "Home"),
                    make("Victim", "Quarry", "teamPlayer_2", "Near"),
                    make("Ring", "Quarry", "teamPlayer_2", "Off"),
                    make("Field", "Quarry", "teamPlayer_2", "Out"),
                ],
            ),
            after(
                "Order",
                "Field",
                vec![act(
                    "TEAM_ATTACK_NAMED",
                    vec![text("teamPlayer_1"), text("Victim")],
                )],
            ),
        ],
        20,
    );
    let (walker, victim) = (named(&sim, "Walker"), named(&sim, "Victim"));
    let (ring, field) = (named(&sim, "Ring"), named(&sim, "Field"));
    let mut after_victim = None;
    for _ in 0..300 {
        tick(&mut sim, 1);
        after_victim = after_victim.or(attack_target(&sim, walker).filter(|t| *t != victim));
    }
    assert_eq!(after_victim, Some(field), "ring foe is {ring:?}");
}

#[test]
fn a_melee_unit_in_the_ring_reacquires_after_a_kill() {
    assert!(reacquires("Rim3"));
    assert!(!reacquires("Lip"));
}

fn reacquires(next_at: &str) -> bool {
    let mut sim = edge_wilds_run(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Walker", "Brawler", "teamWilds", "Rim"),
                    make("Victim", "Fragile", "teamPlayer_1", "Rim2"),
                    make("Next", "Quarry", "teamPlayer_1", next_at),
                ],
            ),
            after(
                "Order",
                "Next",
                vec![act(
                    "TEAM_ATTACK_NAMED",
                    vec![text("teamWilds"), text("Victim")],
                )],
            ),
        ],
        20,
    );
    let ids = sim.rules().ids().unwrap();
    let (walker, next) = (named(&sim, "Walker"), named(&sim, "Next"));
    let mut reacquired = false;
    for _ in 0..300 {
        tick(&mut sim, 1);
        reacquired |= sim
            .state()
            .get(ids.armed, walker)
            .is_some_and(|armed| armed.target == Some(next));
    }
    reacquired
}

fn timer_move(unit: &str, to: &str) -> Vec<Script> {
    vec![
        script(
            "Arm",
            None,
            vec![act("SET_TIMER", vec![text("T"), number(10)])],
        ),
        script(
            "Move",
            Some(("TIMER_EXPIRED", vec![text("T")])),
            vec![act("UNIT_TELEPORT_TO_WAYPOINT", vec![text(unit), text(to)])],
        ),
    ]
}

fn object_guard_scripts(extra: Vec<Script>) -> Vec<Script> {
    let mut scripts = vec![
        script(
            "Make",
            None,
            vec![
                make("Ward", "Spearman", "teamPlayer_1", "WardA"),
                act("UNIT_SET_TEAM", vec![text("Ward"), text("teamWard")]),
                make("Guarder", "Spearman", "teamPlayer_1", "PostNear"),
            ],
        ),
        after(
            "Guard",
            "Guarder",
            vec![act(
                "TEAM_GUARD_OBJECT",
                vec![text("teamPlayer_1"), text("Ward")],
            )],
        ),
    ];
    scripts.extend(extra);
    scripts
}

fn guarder_after_ward_moves(to: &str) -> Vec<(GuardState, Vec2)> {
    let mut sim = simulate(object_guard_scripts(timer_move("Ward", to)), 30);
    let ids = sim.rules().ids().unwrap();
    let (guarder, ward) = (named(&sim, "Guarder"), named(&sim, "Ward"));
    let target = match to {
        "WardB" => Vec2::new(515.0, 515.0),
        "WardC" => Vec2::new(521.0, 500.0),
        _ => Vec2::new(530.0, 500.0),
    };
    let mut seen = Vec::new();
    for _ in 0..900 {
        tick(&mut sim, 1);
        let at = sim.state().get(ids.body, ward).unwrap().0.position;
        if at == target {
            let guard = sim.state().get(ids.guard, guarder).unwrap();
            seen.push((guard.state, guard.goal));
        }
    }
    seen
}

#[test]
fn an_object_guard_returns_to_a_ward_that_moved() {
    let seen = guarder_after_ward_moves("WardD");
    let target = Vec2::new(530.0, 500.0);
    assert!(
        seen.iter()
            .any(|&(state, goal)| state == GuardState::Return && goal == target),
        "{} ticks after the move, none in RETURN toward {target:?}",
        seen.len()
    );
}

#[test]
fn ward_moves_are_tested_per_axis() {
    let diagonal = guarder_after_ward_moves("WardB");
    let along = guarder_after_ward_moves("WardC");
    assert!(!diagonal.is_empty() && !along.is_empty());
    assert!(diagonal
        .iter()
        .all(|&(state, _)| state != GuardState::Return));
    assert!(along.iter().any(|&(state, _)| state == GuardState::Return));
}

#[test]
fn team_guard_scans_around_the_member_mean() {
    let mut sim = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("WardL", "Spearman", "teamPlayer_1", "WardLeft"),
                    act("UNIT_SET_TEAM", vec![text("WardL"), text("teamWard")]),
                    make("WardR", "Spearman", "teamPlayer_1", "WardRight"),
                    act("UNIT_SET_TEAM", vec![text("WardR"), text("teamWard")]),
                    make("Guarder", "Spearman", "teamPlayer_1", "PostFar"),
                    make("Bait", "Quarry", "teamPlayer_2", "Bait"),
                ],
            ),
            after(
                "Guard",
                "Guarder",
                vec![act(
                    "TEAM_GUARD_TEAM",
                    vec![text("teamPlayer_1"), text("teamWard")],
                )],
            ),
        ],
        30,
    );
    let ids = sim.rules().ids().unwrap();
    let (guarder, bait) = (named(&sim, "Guarder"), named(&sim, "Bait"));
    let mut acquired = false;
    for _ in 0..900 {
        tick(&mut sim, 1);
        acquired |= sim
            .state()
            .get(ids.guard, guarder)
            .is_some_and(|guard| guard.nemesis == Some(bait));
    }
    assert!(acquired);
}

#[test]
fn a_gone_ward_measures_inner_from_the_origin() {
    let mut sim = simulate(
        object_guard_scripts(vec![
            script(
                "Arm",
                None,
                vec![act("SET_TIMER", vec![text("T"), number(10)])],
            ),
            script(
                "Remove",
                Some(("TIMER_EXPIRED", vec![text("T")])),
                vec![
                    act("NAMED_DELETE", vec![text("Ward")]),
                    make("Bait", "Quarry", "teamPlayer_2", "Beside"),
                ],
            ),
        ]),
        30,
    );
    let ids = sim.rules().ids().unwrap();
    let guarder = named(&sim, "Guarder");
    let mut states = Vec::new();
    for _ in 0..900 {
        tick(&mut sim, 1);
        if let Some(guard) = sim.state().get(ids.guard, guarder) {
            states.push(guard.state);
        }
    }
    assert!(states.contains(&GuardState::Inner), "never engaged");
    assert!(states
        .windows(2)
        .any(|pair| pair == [GuardState::Inner, GuardState::Outer]));
}

#[test]
fn the_guard_object_and_guard_team_rows_set_their_target() {
    let object = simulate(object_guard_scripts(Vec::new()), 40);
    let ids = object.rules().ids().unwrap();
    let guard = object
        .state()
        .get(ids.guard, named(&object, "Guarder"))
        .unwrap();
    assert_eq!(guard.target, GuardTarget::Object(named(&object, "Ward")));
    assert!(guard.scripted && guard.mode == GuardMode::Normal);

    let team = simulate(
        vec![
            script(
                "Make",
                None,
                vec![
                    make("Ward", "Spearman", "teamPlayer_1", "WardA"),
                    act("UNIT_SET_TEAM", vec![text("Ward"), text("teamWard")]),
                    make("Guarder", "Spearman", "teamPlayer_1", "PostNear"),
                ],
            ),
            after(
                "Guard",
                "Guarder",
                vec![act(
                    "TEAM_GUARD_TEAM",
                    vec![text("teamPlayer_1"), text("teamWard")],
                )],
            ),
        ],
        40,
    );
    let guard = team
        .state()
        .get(ids.guard, named(&team, "Guarder"))
        .unwrap();
    assert!(matches!(guard.target, GuardTarget::Team(_)));
}
