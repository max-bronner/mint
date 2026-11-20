use super::*;
use crate::templates::KindTable;
use mint_sage_map::{Dictionary, HeightMapData, OrCondition, ScriptTree, TeamDef, Value};
use mint_sage_passability::Passability;
use mint_sim::Simulation;

fn noop(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionEnable(ScriptName {
        name: String::new(),
        side: String::new(),
    })))
}

fn parse_in_mode(registry: &[ActionRow], name: &str, run_mode: u8) -> ParsedAction {
    let (waypoints, areas, cameras) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let ctx = ParseCtx {
        waypoints: &waypoints,
        trigger_areas: &areas,
        allow_ring_heroes: false,
        named_cameras: &cameras,
        side: "",
        run_mode,
        record_seq: &std::cell::Cell::new(0),
    };
    let action = ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params: Vec::new(),
    };
    parse_action_in(registry, &action, &ctx)
}

#[test]
fn an_action_runs_only_in_a_mode_its_mask_names() {
    let registry = [
        ActionRow::new("EVERY_MODE", noop, MODE_ENGINE),
        ActionRow::new("STANDARD_ONLY", noop, MODE_STANDARD),
        ActionRow::new("OTHER_MODE_ONLY", noop, 2),
    ];
    let runs = |name, mode| matches!(parse_in_mode(&registry, name, mode), ParsedAction::Verb(_));
    assert!(runs("EVERY_MODE", RUN_MODE));
    assert!(runs("STANDARD_ONLY", RUN_MODE));
    assert!(!runs("OTHER_MODE_ONLY", RUN_MODE));
    assert!(runs("OTHER_MODE_ONLY", 2));
    assert!(!runs("STANDARD_ONLY", 2));
    assert!(matches!(
        parse_in_mode(&registry, "NOT_A_ROW", RUN_MODE),
        ParsedAction::Unsupported
    ));
}

struct TestTeamHasUnits {
    team: String,
    side: String,
}
impl ConditionVerb for TestTeamHasUnits {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .peek_team(state, team::THIS_TEAM, &self.side)
            .is_some_and(|team| rules.team_has_units(tick, ids, state, team))
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

fn parse_test_team_has_units(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(TestTeamHasUnits {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
    }))
}

struct TestNewInstanceWith {
    team: String,
    unit: String,
}
impl ActionVerb for TestNewInstanceWith {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let (Some(proto), Some(entity)) = (
            engine.team_protos.find("", &self.team),
            rules.named_entity(tick, ids, &self.unit),
        ) else {
            return Ok(());
        };
        let instance = state.rt.teams.create_instance(proto, true);
        state.rt.teams.add_member(instance, entity);
        Ok(())
    }
}

fn parse_test_new_instance(params: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(TestNewInstanceWith {
        team: parameter_string(params, 0)?,
        unit: parameter_string(params, 1)?,
    }))
}

struct TestAlwaysTrue;
impl ConditionVerb for TestAlwaysTrue {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        _: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        true
    }
}

fn parse_test_always_true(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(TestAlwaysTrue))
}

pub(super) const TEST_CONDITIONS: &[ConditionRow] = &[
    ConditionRow {
        name: "TEST_TEAM_HAS_UNITS",
        parse: parse_test_team_has_units,
    },
    ConditionRow {
        name: "TEST_ALWAYS_TRUE",
        parse: parse_test_always_true,
    },
];

pub(super) const TEST_ACTIONS: &[ActionRow] = &[ActionRow::new(
    "TEST_NEW_INSTANCE_WITH",
    parse_test_new_instance,
    MODE_STANDARD,
)];

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

fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

fn script_of(
    name: &str,
    active: bool,
    one_shot: bool,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
) -> Script {
    let (condition_name, params) = condition.unwrap_or(("CONDITION_TRUE", Vec::new()));
    Script {
        name: name.to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active,
        one_shot,
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

fn team_def(name: &str, singleton: bool, pairs: Vec<(&str, Value)>) -> TeamDef {
    TeamDef {
        owner: String::new(),
        name: name.to_owned(),
        singleton,
        properties: Dictionary::from_pairs(
            pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        ),
    }
}

const UNIT_DATA: &str = "\
Object ZSoldier\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

fn spawn(unit: &str, team: &str, at: &str) -> ScriptAction {
    act(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![text(unit), text("ZSoldier"), text(team), text(at)],
    )
}

fn rules_over(width: u32) -> RotwkRules {
    let map = HeightMapData {
        width,
        height: width,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; (width * width) as usize],
    };
    RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new())
}

fn run_state(scripts: Vec<Script>, teams: &[TeamDef], ticks: u32) -> MapScriptState {
    run_slots(
        vec![scripts.into_iter().map(ScriptNode::Script).collect()],
        teams,
        ticks,
    )
}

fn run_slots(slots: Vec<Vec<ScriptNode>>, teams: &[TeamDef], ticks: u32) -> MapScriptState {
    let data = crate::game::tests::data_from(UNIT_DATA);
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZSoldier")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        (String::from("P1"), (Vec2::new(10.0, 10.0), 0.0)),
        (String::from("P2"), (Vec2::new(20.0, 30.0), 0.0)),
    ]
    .into_iter()
    .collect();
    let tree = ScriptTree {
        player_slots: slots,
    };
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &BTreeMap::new(), false)
        .with_team_prototypes(TeamProtos::from_defs(teams).unwrap());
    let rules = rules_over(96)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    let state = world
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()));
    state.unwrap()
}

#[test]
fn a_script_naming_a_team_is_evaluated_once_per_instance_of_it() {
    let setup = script_of(
        "Setup",
        true,
        true,
        None,
        vec![
            spawn("UnitA", "teamWilds", "P1"),
            spawn("UnitB", "teamWilds", "P2"),
            act(
                "TEST_NEW_INSTANCE_WITH",
                vec![text("teamWilds"), text("UnitB")],
            ),
        ],
    );
    let count = script_of(
        "Count",
        true,
        true,
        Some(("TEST_TEAM_HAS_UNITS", vec![text("teamWilds")])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("hits")])],
    );
    let wave = team_def(
        "teamWilds",
        false,
        vec![("teamMaxInstances", Value::Int(2))],
    );
    let state = run_state(vec![setup, count], &[wave], 8);
    assert_eq!(state.counter_value("", "hits"), 2);
}

#[test]
fn a_team_on_create_script_runs_once_when_the_team_is_activated() {
    let setup = script_of(
        "Setup",
        true,
        true,
        None,
        vec![spawn("UnitA", "teamWilds", "P1")],
    );
    let mut on_create = script_of(
        "WhenCreated",
        true,
        false,
        Some(("TEST_TEAM_HAS_UNITS", vec![text(team::THIS_TEAM)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("created")])],
    );
    on_create.subroutine = true;
    let solo = team_def(
        "teamWilds",
        true,
        vec![("teamOnCreateScript", Value::Str("WhenCreated".to_owned()))],
    );
    let state = run_state(vec![setup, on_create], &[solo], 20);
    assert_eq!(state.counter_value("", "created"), 1);
}

#[test]
fn a_team_on_create_hook_naming_a_non_subroutine_runs_nothing() {
    let setup = script_of(
        "Setup",
        true,
        true,
        None,
        vec![spawn("UnitA", "teamWilds", "P1")],
    );
    let on_create = script_of(
        "WhenCreated",
        true,
        false,
        Some(("TEST_TEAM_HAS_UNITS", vec![text(team::THIS_TEAM)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("created")])],
    );
    let solo = team_def(
        "teamWilds",
        true,
        vec![("teamOnCreateScript", Value::Str("WhenCreated".to_owned()))],
    );
    let state = run_state(vec![setup, on_create], &[solo], 20);
    assert_eq!(state.counter_value("", "created"), 0);
}

#[test]
fn a_team_on_create_script_whose_conditions_fail_runs_none_of_its_actions() {
    let setup = script_of(
        "Setup",
        true,
        true,
        None,
        vec![spawn("UnitA", "teamWilds", "P1")],
    );
    let mut on_create = script_of(
        "WhenCreated",
        true,
        false,
        Some(("FLAG", vec![text("never"), number(1)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("created")])],
    );
    on_create.subroutine = true;
    let solo = team_def(
        "teamWilds",
        true,
        vec![("teamOnCreateScript", Value::Str("WhenCreated".to_owned()))],
    );
    let state = run_state(vec![setup, on_create], &[solo], 20);
    assert_eq!(state.counter_value("", "created"), 0);
}

fn dispatch_probe(template: &'static str) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionCreateNamed {
        unit_name: String::new(),
        template: template.to_owned(),
        team: String::new(),
        side: String::new(),
        at: None,
    }))
}

fn parse_first(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    dispatch_probe("first")
}

fn parse_second(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    dispatch_probe("second")
}

fn parse_with(
    registry: &[ActionRow],
    action_type: i32,
    name: &str,
    params: Vec<Parameter>,
) -> ParsedAction {
    let (waypoints, areas, cameras) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let ctx = ParseCtx {
        waypoints: &waypoints,
        trigger_areas: &areas,
        allow_ring_heroes: false,
        named_cameras: &cameras,
        side: "",
        run_mode: RUN_MODE,
        record_seq: &std::cell::Cell::new(0),
    };
    let action = ScriptAction {
        action_type,
        enabled: true,
        name: name.to_owned(),
        params,
    };
    parse_action_in(registry, &action, &ctx)
}

fn template_of(parsed: ParsedAction) -> Option<String> {
    match parsed {
        ParsedAction::Verb(verb) => verb.referenced_template().map(str::to_owned),
        _ => None,
    }
}

#[test]
fn a_name_registered_twice_runs_the_row_of_the_template_id_the_file_stored() {
    let registry = [
        ActionRow::new("TWICE", parse_first, MODE_ENGINE).with_id(10),
        ActionRow::new("TWICE", parse_second, MODE_ENGINE).with_id(11),
    ];
    let run = |stored| template_of(parse_with(&registry, stored, "TWICE", Vec::new()));
    assert_eq!(run(11).as_deref(), Some("second"));
    assert_eq!(run(10).as_deref(), Some("first"));
    assert_eq!(run(99).as_deref(), Some("first"));
}

fn parse_status_probe(params: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    let template = parameter_int(params, 0)?.to_string();
    Some(Arc::new(ActionCreateNamed {
        unit_name: String::new(),
        template,
        team: String::new(),
        side: String::new(),
        at: None,
    }))
}

#[test]
fn a_status_parameter_reaches_its_verb_as_the_index_of_the_name_it_stores() {
    let registry = [ActionRow::new("PROBE", parse_status_probe, MODE_ENGINE)];
    let status = Parameter::Value {
        param_type: 41,
        int_value: 0,
        real_value: 0.0,
        string_value: "Is_Attacking".to_owned(),
    };
    let parsed = parse_with(&registry, 0, "PROBE", vec![status]);
    assert_eq!(template_of(parsed).as_deref(), Some("22"));
}

#[test]
fn player_tokens_resolve_to_masks_over_the_match_relations() {
    let rules = rules_over(8)
        .with_players(3)
        .with_teams(vec![Some(0), Some(0), Some(1)])
        .with_map_players(
            3,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_script_local_player(Some(PlayerId::new(2)));
    let mut runtime = runtime::ScriptRuntime::default();
    let ids_of = |runtime: &runtime::ScriptRuntime, text: &str| -> Vec<u8> {
        rules
            .player_mask(runtime, text, Some(PlayerId::new(0)))
            .players()
            .map(PlayerId::get)
            .collect()
    };
    assert_eq!(ids_of(&runtime, "<This Player's Enemies>"), [2]);
    assert_eq!(ids_of(&runtime, "<This Player's Allies>"), [1]);
    assert_eq!(ids_of(&runtime, "<This Player's Allies incl Self>"), [0, 1]);
    assert_eq!(ids_of(&runtime, "<This Player>"), [0]);
    assert_eq!(ids_of(&runtime, "<Local Player>"), [2]);
    assert_eq!(ids_of(&runtime, "<Local Player's Enemies>"), [0, 1]);
    assert_eq!(ids_of(&runtime, "<All Players>"), [0, 1, 2, 3, 4]);
    assert_eq!(ids_of(&runtime, "Wilds"), [3]);
    assert!(ids_of(&runtime, "NoSuchPlayer").is_empty());
    runtime.relations.set_player_player(
        PlayerId::new(0),
        PlayerId::new(2),
        relations::Relation::Allies,
    );
    assert!(ids_of(&runtime, "<This Player's Enemies>").is_empty());
    assert_eq!(ids_of(&runtime, "<This Player's Allies>"), [1, 2]);
}

#[test]
fn the_replay_observer_joins_all_players_only_while_the_list_has_room() {
    let all = |map_players: usize| -> Vec<u8> {
        let rules = rules_over(8).with_players(2).with_map_players(
            2,
            vec![String::new(); map_players],
            vec![crate::alliance::MapRelations::default(); map_players],
        );
        rules
            .player_mask(
                &runtime::ScriptRuntime::default(),
                "<All Players>",
                Some(PlayerId::new(0)),
            )
            .players()
            .map(PlayerId::get)
            .collect()
    };
    assert_eq!(all(17), (0..20).collect::<Vec<u8>>());
    assert_eq!(all(18), (0..20).collect::<Vec<u8>>());
    assert_eq!(all(3), (0..6).collect::<Vec<u8>>());
}

mod audio;
mod verbs_s3;

#[test]
fn a_sides_groups_run_before_the_next_sides_scripts() {
    let setter = script_of(
        "SetIt",
        true,
        false,
        None,
        vec![
            act("SET_FLAG", vec![text("go"), number(1)]),
            act("INCREMENT_COUNTER", vec![number(1), text("passes")]),
        ],
    );
    let group = ScriptNode::Group(mint_sage_map::ScriptGroup {
        name: String::from("First"),
        active: true,
        is_subroutine: false,
        children: vec![ScriptNode::Script(setter)],
    });
    let reader = script_of(
        "Read",
        true,
        false,
        Some(("FLAG", vec![text("go"), number(1)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("seen")])],
    );
    let state = run_slots(vec![vec![group], vec![ScriptNode::Script(reader)]], &[], 30);
    let passes = state.counter_value("", "passes");
    assert!(passes > 0);
    assert_eq!(state.counter_value("", "seen"), passes);
}

#[test]
fn a_disabled_condition_counts_as_true() {
    let mut disabled = script_of(
        "Disabled",
        true,
        false,
        Some(("FLAG", vec![text("never"), number(1)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("disabled")])],
    );
    disabled.or_conditions = vec![OrCondition {
        conditions: vec![Condition {
            enabled: false,
            ..disabled.or_conditions[0].conditions[0].clone()
        }],
    }];
    let enabled = script_of(
        "Enabled",
        true,
        false,
        Some(("FLAG", vec![text("never"), number(1)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("enabled")])],
    );
    let state = run_state(vec![disabled, enabled], &[], 30);
    assert!(state.counter_value("", "disabled") > 0);
    assert_eq!(state.counter_value("", "enabled"), 0);
}

#[test]
fn not_negates_a_condition_except_on_the_types_the_engine_evaluates_itself() {
    let counted = |condition: &str, inverted: bool| {
        let mut script = script_of(
            "S",
            true,
            false,
            Some((condition, Vec::new())),
            vec![act("INCREMENT_COUNTER", vec![number(1), text("fired")])],
        );
        script.or_conditions[0].conditions[0].inverted = inverted;
        run_state(vec![script], &[], 30).counter_value("", "fired") > 0
    };
    assert!(counted("TEST_ALWAYS_TRUE", false));
    assert!(!counted("TEST_ALWAYS_TRUE", true));
    assert!(counted("CONDITION_TRUE", true));
}

#[test]
fn a_disabled_action_is_skipped_while_its_script_still_fires() {
    let mut off = act("INCREMENT_COUNTER", vec![number(1), text("off")]);
    off.enabled = false;
    let on = act("INCREMENT_COUNTER", vec![number(1), text("on")]);
    let state = run_state(
        vec![script_of("S", true, false, None, vec![off, on])],
        &[],
        30,
    );
    assert_eq!(state.counter_value("", "off"), 0);
    assert!(state.counter_value("", "on") > 0);
}

#[test]
fn a_script_without_a_condition_term_never_fires() {
    let mut no_term = script_of(
        "NoTerm",
        true,
        false,
        None,
        vec![act("INCREMENT_COUNTER", vec![number(1), text("no_term")])],
    );
    no_term.or_conditions.clear();
    let mut empty_term = script_of(
        "EmptyTerm",
        true,
        false,
        None,
        vec![act(
            "INCREMENT_COUNTER",
            vec![number(1), text("empty_term")],
        )],
    );
    empty_term.or_conditions = vec![OrCondition::default()];
    let mut skipped_term = script_of(
        "SkippedTerm",
        true,
        false,
        Some(("FLAG", vec![text("never"), number(1)])),
        vec![act(
            "INCREMENT_COUNTER",
            vec![number(1), text("skipped_term")],
        )],
    );
    skipped_term.or_conditions.insert(0, OrCondition::default());
    let control = script_of(
        "Control",
        true,
        false,
        None,
        vec![act("INCREMENT_COUNTER", vec![number(1), text("control")])],
    );
    let state = run_state(vec![no_term, empty_term, skipped_term, control], &[], 30);
    assert!(state.counter_value("", "control") > 0);
    assert_eq!(state.counter_value("", "no_term"), 0);
    assert_eq!(state.counter_value("", "empty_term"), 0);
    assert_eq!(state.counter_value("", "skipped_term"), 0);
}
