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
use crate::rules::RotwkRules;
use crate::templates::KindTable;

pub(super) const DATA: &str = "\
SpecialPower SpecialAbilityZap\n  ReloadTime = 30000\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon ZapWeapon\n  AttackRange = 500\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n    Radius = 20\n  End\nEnd\n\
CommandSet ZapSet\n  1 = Fire_Zap\n  2 = Toggle_Stuff\nEnd\n\
CommandSet OtherSet\n  1 = Fire_Other\nEnd\n\
CommandButton Fire_Zap\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityZap\nEnd\n\
CommandButton Fire_Other\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityZap\nEnd\n\
Object Zapper\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = ZapSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Zap\n    SpecialPowerTemplate = SpecialAbilityZap\n    \
    SpecialWeapon = ZapWeapon\n    WhichSpecialWeapon = 1\n    UnpackTime = 1\n    \
    PreparationTime = 1\n    PersistentCount = 1\n    PersistentPrepTime = 2000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Bystander\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = OtherSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = WeaponFireSpecialAbilityUpdate Tag_Zap\n    SpecialPowerTemplate = SpecialAbilityZap\n    \
    SpecialWeapon = ZapWeapon\n    WhichSpecialWeapon = 1\n    UnpackTime = 1\n    \
    PreparationTime = 1\n    PersistentCount = 1\n    PersistentPrepTime = 2000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
StanceTemplate PlainStances\n  Stance Battle\n  End\n  Stance HoldGround\n  End\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = StancesBehavior Tag_Stance\n    StanceTemplate = PlainStances\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
  Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 10\n    TriggerWidthY = 10\n  End\n  \
  Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
    ResetTimeInMilliseconds = 1000\n  End\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n";

pub(super) const P0: PlayerId = PlayerId::new(0);
pub(super) const P1: PlayerId = PlayerId::new(1);

pub(super) fn text(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

pub(super) fn number(value: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: value,
        real_value: 0.0,
        string_value: String::new(),
    }
}

pub(super) fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        name: name.to_owned(),
        params,
    }
}

pub(super) fn script(
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

pub(super) struct Placed {
    pub(super) kind: &'static str,
    pub(super) name: &'static str,
    pub(super) at: (f32, f32),
    pub(super) owner: PlayerId,
}

pub(super) fn placed(
    kind: &'static str,
    name: &'static str,
    at: (f32, f32),
    owner: PlayerId,
) -> Placed {
    Placed {
        kind,
        name,
        at,
        owner,
    }
}

pub(super) fn simulate(
    data_text: &str,
    objects: &[Placed],
    scripts: Vec<Script>,
    ticks: u32,
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(data_text);
    let mut kind_names: Vec<String> = objects.iter().map(|o| o.kind.to_owned()).collect();
    kind_names.push("Bystander".to_owned());
    kind_names.sort();
    kind_names.dedup();
    let (kinds, missing) = KindTable::closure(&data, &kind_names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        ("Home", (Vec2::new(60.0, 60.0), 0.0)),
        ("Far", (Vec2::new(400.0, 60.0), 0.0)),
    ]
    .into_iter()
    .map(|(name, at)| (name.to_owned(), at))
    .collect();
    let areas: BTreeMap<String, Vec<(f32, f32)>> = BTreeMap::new();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let protos = TeamProtos::from_defs(&[TeamDef {
        owner: "Player_1".to_owned(),
        name: "teamPlayer_1".to_owned(),
        singleton: true,
        properties: Dictionary::from_pairs(Vec::new()),
    }])
    .unwrap();
    let engine = MapScriptEngine::build(
        &[tree],
        &[vec!["Player_1".to_owned()]],
        &waypoints,
        &areas,
        false,
    )
    .with_team_prototypes(protos);
    let map = HeightMapData {
        width: 160,
        height: 160,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; 160 * 160],
    };
    let spawns: Vec<_> = objects
        .iter()
        .map(|o| {
            let kind = kinds.kind(o.kind).unwrap();
            let at = Vec2::new(o.at.0, o.at.1);
            kinds
                .spawn(&data.combat, kind, at, Facing::EAST, o.owner)
                .unwrap()
        })
        .collect();
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let names = objects
        .iter()
        .enumerate()
        .map(|(index, o)| (index, o.name.to_owned()))
        .collect();
    let rules = RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), spawns)
        .with_script_object_names(names)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(Some(engine), vec![(0, P0), (1, P1)])
        .with_script_local_player(Some(P1))
        .with_combat(data.combat.clone())
        .with_stances(Arc::clone(&data.stances))
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_ai_power_buttons(Arc::clone(&data.command_buttons))
        .with_command_sets(Arc::clone(&data.command_sets));
    let mut simulation = Simulation::new(5, rules).unwrap();
    tick(&mut simulation, ticks);
    simulation
}

pub(super) fn tick(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

pub(super) fn named(simulation: &Simulation<RotwkRules>, name: &str) -> Entity {
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
        .unwrap_or_else(|| panic!("{name} was not placed"))
}

pub(super) fn state_of(simulation: &Simulation<RotwkRules>) -> MapScriptState {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

fn gate_open(simulation: &Simulation<RotwkRules>, gate: Entity) -> Option<bool> {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .rules()
        .gate_is_open(simulation.state(), ids, gate)
}

pub(super) fn available_on(simulation: &Simulation<RotwkRules>, caster: Entity) -> u64 {
    let ids = simulation.rules().ids().unwrap();
    let power = simulation
        .rules()
        .power_names
        .id("SpecialAbilityZap")
        .unwrap();
    simulation
        .state()
        .get(ids.power_book, caster)
        .and_then(|book| book.instance(power))
        .map_or(0, |instance| instance.available_on.get())
}

const GATE_TICKS: u32 = 90;

fn gate_scripts(open_then_close: bool) -> Vec<Script> {
    let mut scripts = vec![script(
        "Open",
        None,
        vec![
            act("GATE_OPEN", vec![text("TheGate")]),
            act("SET_TIMER", vec![text("later"), number(15)]),
        ],
    )];
    if open_then_close {
        scripts.push(script(
            "Close",
            Some(("TIMER_EXPIRED", vec![text("later")])),
            vec![act("GATE_CLOSE", vec![text("TheGate")])],
        ));
    }
    scripts
}

#[test]
fn gate_open_and_gate_close_move_the_named_gate() {
    let objects = [placed("Gate", "TheGate", (50.0, 50.0), P0)];
    let opened = simulate(DATA, &objects, gate_scripts(false), GATE_TICKS);
    assert_eq!(gate_open(&opened, named(&opened, "TheGate")), Some(true));
    let closed = simulate(DATA, &objects, gate_scripts(true), GATE_TICKS * 3);
    assert_eq!(gate_open(&closed, named(&closed, "TheGate")), Some(false));
}

fn fire_at_home() -> ScriptAction {
    act(
        "PLAYER_FIRE_SPECIAL_POWER_AT_WAYPOINT",
        vec![text("Player_1"), text("SpecialAbilityZap"), text("Home")],
    )
}

fn power_scripts() -> Vec<Script> {
    vec![
        script(
            "First",
            None,
            vec![
                fire_at_home(),
                act("SET_TIMER", vec![text("again"), number(5)]),
            ],
        ),
        script(
            "Second",
            Some(("TIMER_EXPIRED", vec![text("again")])),
            vec![fire_at_home()],
        ),
        script(
            "Saw",
            Some((
                "PLAYER_TRIGGERED_SPECIAL_POWER",
                vec![text("Player_1"), text("SpecialAbilityZap")],
            )),
            vec![act("SET_FLAG", vec![text("saw"), number(1)])],
        ),
    ]
}

fn zap_objects() -> [Placed; 2] {
    [
        placed("Zapper", "ZapA", (50.0, 50.0), P0),
        placed("Zapper", "ZapB", (60.0, 50.0), P0),
    ]
}

#[test]
fn a_scripted_power_fires_the_first_caster_even_while_recharging() {
    let first = simulate(DATA, &zap_objects(), power_scripts(), 20);
    let (a1, b1) = (
        available_on(&first, named(&first, "ZapA")),
        available_on(&first, named(&first, "ZapB")),
    );
    assert!(a1 > 0, "the first caster recharges after firing");
    assert_eq!(b1, 0, "only the first caster fires");
    let second = simulate(DATA, &zap_objects(), power_scripts(), 60);
    let a2 = available_on(&second, named(&second, "ZapA"));
    assert!(
        a2 > a1,
        "firing again mid-recharge restarts it: {a1} then {a2}"
    );
    assert!(
        state_of(&second).flags.values().any(|set| *set),
        "PLAYER_TRIGGERED_SPECIAL_POWER saw the firing"
    );
}

fn press(unit: &str, button: &str) -> Vec<Script> {
    vec![script(
        "Press",
        None,
        vec![act(
            "NAMED_USE_COMMANDBUTTON_ABILITY",
            vec![text(unit), text(button)],
        )],
    )]
}

#[test]
fn a_named_button_press_needs_the_button_in_the_units_own_set() {
    let objects = [placed("Zapper", "ZapA", (50.0, 50.0), P0)];
    let own = simulate(DATA, &objects, press("ZapA", "Fire_Zap"), 20);
    assert!(available_on(&own, named(&own, "ZapA")) > 0);
    let foreign = simulate(DATA, &objects, press("ZapA", "Fire_Other"), 20);
    assert_eq!(available_on(&foreign, named(&foreign, "ZapA")), 0);
}

fn spawn_at(x: f32, y: f32) -> ScriptAction {
    act(
        "UNIT_SPAWN_NAMED_LOCATION_ORIENTATION",
        vec![
            text("Born"),
            text("Bystander"),
            text("teamPlayer_1"),
            Parameter::Coord3D { x, y, z: 0.0 },
            Parameter::Value {
                param_type: 0,
                int_value: 0,
                real_value: 0.0,
                string_value: String::new(),
            },
        ],
    )
}

#[test]
fn spawning_under_a_live_name_makes_nothing() {
    let scripts = vec![
        script(
            "First",
            None,
            vec![
                spawn_at(100.0, 100.0),
                act("SET_TIMER", vec![text("again"), number(3)]),
            ],
        ),
        script(
            "Second",
            Some(("TIMER_EXPIRED", vec![text("again")])),
            vec![spawn_at(200.0, 200.0)],
        ),
    ];
    let sim = simulate(DATA, &[], scripts, 60);
    let ids = sim.rules().ids().unwrap();
    let born: Vec<Entity> = sim
        .state()
        .store(ids.script_name)
        .unwrap()
        .iter()
        .filter(|(_, tag)| tag.0 == "Born")
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(born.len(), 1);
    let at = sim.state().get(ids.body, born[0]).unwrap().0.position;
    assert!((at.x - 100.0).abs() < 1.0, "{at:?}");
}

#[test]
fn named_attack_named_orders_the_attacker_onto_the_target() {
    let objects = [
        placed("Soldier", "Hitter", (50.0, 50.0), P0),
        placed("Bystander", "Victim", (300.0, 50.0), P1),
    ];
    let scripts = vec![script(
        "Go",
        None,
        vec![act(
            "NAMED_ATTACK_NAMED",
            vec![text("Hitter"), text("Victim")],
        )],
    )];
    let sim = simulate(DATA, &objects, scripts, 12);
    let ids = sim.rules().ids().unwrap();
    let order = sim
        .state()
        .get(ids.attack_order, named(&sim, "Hitter"))
        .copied();
    assert_eq!(
        order.map(|o| o.target),
        Some(named(&sim, "Victim")),
        "{order:?}"
    );
}

const NEAREST_DATA: &str = "\
Object Marker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildVariations = DummyB\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object DummyB\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Other\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn fire_on_nearest_object_type_hits_the_nearest_equivalent_object_to_the_team() {
    let data = format!("{DATA}{NEAREST_DATA}");
    let objects = [
        placed("Zapper", "Caster", (400.0, 30.0), P0),
        placed("Marker", "Spare", (400.0, 0.0), P0),
        placed("DummyB", "Near", (400.0, 140.0), P1),
        placed("Dummy", "Farther", (150.0, 60.0), P1),
        placed("Other", "Decoy", (440.0, 60.0), P1),
    ];
    let scripts = vec![
        script(
            "Make",
            None,
            vec![act(
                "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
                vec![
                    text("Anchor"),
                    text("Marker"),
                    text("teamPlayer_1"),
                    text("Far"),
                ],
            )],
        ),
        script(
            "Fire",
            Some(("NAMED_CREATED", vec![text("Anchor")])),
            vec![act(
                "FIRE_SPECIAL_POWER_ON_NEAREST_OBJECTTYPE",
                vec![
                    text("Player_1"),
                    text("SpecialAbilityZap"),
                    text("Dummy"),
                    text("teamPlayer_1"),
                ],
            )],
        ),
    ];
    let sim = simulate(&data, &objects, scripts, 60);
    let ids = sim.rules().ids().unwrap();
    let health = |name: &str| {
        sim.state()
            .get(ids.health, named(&sim, name))
            .map_or(0.0, |h| h.0.current())
    };
    let (near, farther, decoy) = (health("Near"), health("Farther"), health("Decoy"));
    assert!(near < 100.0, "the nearest equivalent object is hit: {near}");
    assert_eq!((farther, decoy), (100.0, 100.0), "nothing else is hit");
}

const FRAGILE_DATA: &str = "\
Weapon Maul\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
Object Smasher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Maul\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fragile\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DeathTypes = ALL\n    DestructionDelay = 5000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_named_object_dying_of_its_first_blow_was_attacked_by_the_player() {
    let data = format!("{DATA}{FRAGILE_DATA}");
    let objects = [
        placed("Smasher", "Hitter", (50.0, 50.0), P0),
        placed("Fragile", "Victim", (80.0, 50.0), P1),
    ];
    let scripts = vec![
        script(
            "Go",
            None,
            vec![act(
                "NAMED_ATTACK_NAMED",
                vec![text("Hitter"), text("Victim")],
            )],
        ),
        script(
            "Seen",
            Some((
                "NAMED_ATTACKED_BY_PLAYER",
                vec![text("Victim"), text("Player_1")],
            )),
            vec![act("SET_COUNTER", vec![text("/hit"), number(1)])],
        ),
    ];
    let sim = simulate(&data, &objects, scripts, 90);
    assert_eq!(state_of(&sim).counter_value("", "hit"), 1);
}

const LAB: &str = "\
Upgrade Upgrade_Lab\n  Type = OBJECT\n  BuildCost = 10\n  BuildTime = 1\nEnd\n\
CommandButton Buy_Lab\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Lab\nEnd\n\
CommandSet LabSet\n  1 = Buy_Lab\nEnd\n\
Object Lab\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = LabSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Lab\n    TriggeredBy = Upgrade_Lab\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n";

#[test]
fn a_researched_object_upgrade_satisfies_player_built_upgrade_once() {
    let data = format!("{DATA}{LAB}");
    let mut built = script(
        "Built",
        Some((
            "PLAYER_BUILT_UPGRADE",
            vec![text("Player_1"), text("Upgrade_Lab")],
        )),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("built")])],
    );
    built.one_shot = false;
    let mut simulation = simulate(
        &data,
        &[placed("Lab", "Lab", (80.0, 80.0), P0)],
        vec![built],
        1,
    );
    let lab = named(&simulation, "Lab");
    simulation
        .submit(
            P0,
            crate::rules::RotwkCommand::SetSelection {
                replace: true,
                units: vec![lab],
            },
        )
        .unwrap();
    simulation
        .submit(P0, crate::rules::RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    tick(&mut simulation, 60);
    let counted: i32 = state_of(&simulation)
        .counters
        .values()
        .map(|counter| counter.value)
        .sum();
    assert_eq!(counted, 1, "one research, one firing");
}
