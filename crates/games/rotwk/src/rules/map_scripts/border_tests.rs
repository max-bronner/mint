use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, Passability as _, Vec2};
use mint_sage_map::{
    Boundary, Condition, HeightMapData, OrCondition, Parameter, Script, ScriptAction, ScriptNode,
    ScriptTree,
};
use mint_sage_passability::Passability;
use mint_sim::{PlayerId, Simulation};

use super::MapScriptEngine;
use crate::economy::ComputerSide;
use crate::rules::{Ids, RotwkRules};
use crate::templates::KindTable;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZRunner\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

const HUMAN: PlayerId = PlayerId::new(0);
const COMPUTER: PlayerId = PlayerId::new(2);

fn number(index: i32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: index,
        real_value: 0.0,
        string_value: String::new(),
    }
}

fn text(value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: 0,
        real_value: 0.0,
        string_value: value.to_owned(),
    }
}

fn script(name: &str, timer: Option<&str>, actions: Vec<ScriptAction>) -> Script {
    let (condition, params) = match timer {
        Some(timer) => ("TIMER_EXPIRED", vec![text(timer)]),
        None => ("CONDITION_TRUE", Vec::new()),
    };
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
                name: condition.to_owned(),
                params,
                enabled: true,
            }],
        }],
        actions,
        actions_false: Vec::new(),
        sequence: Default::default(),
    }
}

fn switch(name: &str, timer: &str, indices: &[i32]) -> Script {
    script(
        name,
        Some(timer),
        indices
            .iter()
            .map(|index| ScriptAction {
                action_type: 0,
                enabled: true,
                name: "MAP_SWITCH_BORDER".to_owned(),
                params: vec![number(*index)],
            })
            .collect(),
    )
}

fn simulation() -> Simulation<RotwkRules> {
    let arm = |timer: &str, frames: i32| ScriptAction {
        action_type: 0,
        enabled: true,
        name: "SET_TIMER".to_owned(),
        params: vec![text(timer), number(frames)],
    };
    start(
        vec![
            script(
                "Arm",
                None,
                vec![arm("t1", 10), arm("t2", 40), arm("t3", 80)],
            ),
            switch("First", "t1", &[1]),
            switch("Refused", "t2", &[5, 1]),
            switch("Back", "t3", &[0]),
        ],
        BTreeMap::new(),
        |_| {},
    )
}

fn start(
    scripts: Vec<Script>,
    areas: BTreeMap<String, Vec<(f32, f32)>>,
    paint: impl FnOnce(&mut Passability),
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(&data, &["ZRunner".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let kind = kinds.kind("ZRunner").unwrap();
    let spawns = [HUMAN, COMPUTER]
        .into_iter()
        .map(|owner| {
            kinds
                .spawn(
                    &data.combat,
                    kind,
                    Vec2::new(150.0, 100.0),
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let engine = MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Wilds")]],
        &BTreeMap::new(),
        &areas,
        false,
    );
    let map = HeightMapData {
        width: 30,
        height: 30,
        border_size: 5,
        boundaries: vec![Boundary { x: 20, y: 20 }, Boundary { x: 12, y: 20 }],
        samples: vec![0; 30 * 30],
    };
    let mut passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    paint(&mut passability);
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_computer_sides(vec![ComputerSide {
            player: COMPUTER,
            evil: true,
            cash: 0,
        }])
        .with_script_local_player(Some(HUMAN))
        .with_map_scripts(Some(engine), vec![(0, HUMAN), (1, PlayerId::new(1))])
        .with_shroud(false)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    Simulation::new(5, rules).unwrap()
}

fn run_to(sim: &mut Simulation<RotwkRules>, script_frame: u64) {
    while sim.frame().get() < script_frame * super::counters::TICKS_PER_SCRIPT_FRAME {
        sim.tick().unwrap();
    }
}

fn ids(sim: &Simulation<RotwkRules>) -> Ids {
    sim.rules().ids().unwrap()
}

fn runner(sim: &Simulation<RotwkRules>, owner: PlayerId) -> mint_sim::Entity {
    let ids = ids(sim);
    sim.state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .find(|(_, held)| held.0 == owner)
        .map(|(entity, _)| entity)
        .unwrap()
}

fn enterable(sim: &Simulation<RotwkRules>, owner: PlayerId, at: Vec2) -> bool {
    let rules = sim.rules();
    let class = rules.mover_class_in(sim.state(), ids(sim), runner(sim, owner));
    let cell = rules.passability.cell_at(at).unwrap();
    let view = mint_sage_passability::DynamicView {
        grid: &rules.passability.grid,
        counts: &[],
        rubble: &[],
        wall_rim: &[],
        tags: &[],
        connect: &[],
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: crate::rules::border::barred_of(sim.state(), ids(sim).human_barred),
    };
    view.is_passable(cell, class)
}

#[test]
fn a_switch_moves_the_off_map_line_and_confines_only_human_movers() {
    let mut sim = simulation();
    sim.tick().unwrap();
    let ids = ids(&sim);
    let inside = Vec2::new(150.0, 100.0);
    assert!(!sim.rules().off_map(sim.state(), ids, inside));
    assert!(
        sim.rules().off_map(sim.state(), ids, Vec2::new(0.0, 100.0)),
        "the low edge is strict"
    );

    run_to(&mut sim, 25);
    let rules = sim.rules();
    assert!(
        rules.off_map(sim.state(), ids, inside),
        "x = 150 is past boundary 1's 120"
    );
    assert!(!rules.off_map(sim.state(), ids, Vec2::new(100.0, 100.0)));
    let goal = Vec2::new(170.0, 50.0);
    let clamped = rules.clamp_move_goal(sim.state(), ids, HUMAN, goal);
    assert!(
        clamped.x <= 120.0 - crate::rules::PARTITION_CELL_SIZE * 0.5,
        "{clamped:?}"
    );
    assert_eq!(
        rules.clamp_move_goal(sim.state(), ids, COMPUTER, goal),
        goal
    );
    assert!(
        !enterable(&sim, HUMAN, inside),
        "a human mover may not enter past the boundary"
    );
    assert!(
        enterable(&sim, COMPUTER, inside),
        "a computer's may, inside the union"
    );
    assert!(enterable(&sim, HUMAN, Vec2::new(100.0, 100.0)));

    run_to(&mut sim, 60);
    assert!(
        sim.rules().off_map(sim.state(), ids, inside),
        "index 5 and the already-active 1 do nothing"
    );

    run_to(&mut sim, 100);
    assert!(!sim.rules().off_map(sim.state(), ids, inside));
    assert!(enterable(&sim, HUMAN, inside));
}

#[test]
fn a_switch_narrows_what_the_shroud_answers_without_losing_what_it_held() {
    let mut sim = simulation();
    let state = |sim: &Simulation<RotwkRules>| {
        let ids = ids(sim);
        crate::shroud::with_grid(sim.state(), RotwkRules::shroud_ids(ids), |grid| {
            let (x, y) = grid.world_to_cell(150.0, 100.0);
            grid.cell_state(HUMAN, x, y)
        })
        .unwrap()
    };
    sim.tick().unwrap();
    assert_ne!(state(&sim), mint_sage_shroud::CellVisibility::Unexplored);
    run_to(&mut sim, 25);
    assert_eq!(state(&sim), mint_sage_shroud::CellVisibility::Unexplored);
    run_to(&mut sim, 100);
    assert_ne!(state(&sim), mint_sage_shroud::CellVisibility::Unexplored);
}

#[test]
fn a_human_impassable_area_bars_only_human_movers_until_it_is_freed() {
    let area = |name: &str, flag: i32| ScriptAction {
        action_type: 0,
        enabled: true,
        name: "HUMAN_IMPASSABLE_AREA".to_owned(),
        params: vec![text(name), number(flag)],
    };
    let arm = |timer: &str, frames: i32| ScriptAction {
        action_type: 0,
        enabled: true,
        name: "SET_TIMER".to_owned(),
        params: vec![text(timer), number(frames)],
    };
    let scripts = vec![
        script(
            "Arm",
            None,
            vec![arm("t1", 10), arm("t2", 40), arm("t3", 80)],
        ),
        script("Bar", Some("t1"), vec![area("Zone", 1), area("Missing", 1)]),
        script("Free", Some("t2"), vec![area("Zone", 0)]),
        script("Unpaint", Some("t3"), vec![area("Painted", 0)]),
    ];
    let square = |x: f32, y: f32| vec![(x, y), (x + 40.0, y), (x + 40.0, y + 40.0), (x, y + 40.0)];
    let areas = BTreeMap::from([
        ("Zone".to_owned(), square(60.0, 60.0)),
        ("Painted".to_owned(), square(180.0, 60.0)),
    ]);
    let painted = Vec2::new(195.0, 75.0);
    let mut sim = start(scripts, areas, |passability| {
        let cell = passability.cell_at(painted).unwrap();
        passability
            .grid
            .add_flags(cell, mint_sage_passability::IMPASSABLE_TO_PLAYERS)
            .unwrap();
    });
    let zone = Vec2::new(75.0, 75.0);
    let outside = Vec2::new(75.0, 175.0);
    sim.tick().unwrap();
    assert!(enterable(&sim, COMPUTER, painted));
    assert!(
        !enterable(&sim, HUMAN, painted),
        "a painted cell already bars a human mover"
    );
    assert!(enterable(&sim, HUMAN, zone));

    run_to(&mut sim, 25);
    assert!(!enterable(&sim, HUMAN, zone));
    assert!(
        enterable(&sim, COMPUTER, zone),
        "a computer mover crosses it"
    );
    assert!(enterable(&sim, HUMAN, outside));

    run_to(&mut sim, 60);
    assert!(enterable(&sim, HUMAN, zone), "false frees it again");

    run_to(&mut sim, 100);
    assert!(
        enterable(&sim, HUMAN, painted),
        "false over a painted cell clears it (shared bit, inferred)"
    );
}
