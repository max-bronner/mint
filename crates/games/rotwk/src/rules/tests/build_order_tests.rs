use super::*;

fn porters(
    owners: [PlayerId; 2],
    teams: Vec<Option<u8>>,
) -> (
    Simulation<RotwkRules>,
    Ids,
    crate::unit::Kind,
    Entity,
    Entity,
) {
    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let porter_at = |x: f32, y: f32, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                porter_kind,
                Vec2::new(x, y),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let simulation = Simulation::new(
        61,
        RotwkRules::new(
            passability,
            vec![
                porter_at(140.0, 200.0, owners[0]),
                porter_at(400.0, 200.0, owners[1]),
            ],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_economy(Vec::new(), 1_000)
        .with_teams(teams),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (
        simulation,
        ids,
        kinds.kind("Shed").unwrap(),
        units[0],
        units[1],
    )
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, units: Vec<Entity>) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
}

fn order_shed(
    simulation: &mut Simulation<RotwkRules>,
    shed: crate::unit::Kind,
    porter: Entity,
    x: f32,
    y: f32,
) {
    let ids = simulation.rules().ids().unwrap();
    let player = simulation.state().get(ids.owner, porter).unwrap().0;
    select(simulation, player, vec![porter]);
    simulation
        .submit(
            player,
            RotwkCommand::Construct {
                kind: shed.0,
                x,
                y,
                angle: 0.0,
            },
        )
        .unwrap();
}

fn stamped(simulation: &Simulation<RotwkRules>, ids: Ids) -> bool {
    simulation
        .state()
        .store(ids.obstacles)
        .and_then(|store| store.entities().first().copied())
        .and_then(|clock| simulation.state().get(ids.obstacles, clock))
        .is_some_and(|layer| layer.counts().iter().any(|count| *count > 0))
}

#[test]
fn an_order_puts_nothing_on_the_ground_but_its_plan_blocks_a_second_order() {
    let (mut simulation, ids, shed, near, far) = porters([PLAYER, PLAYER], Vec::new());
    let before = bodies(&simulation, ids);
    order_shed(&mut simulation, shed, far, 150.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.build_task, far),
        "the order is carried by its builder"
    );
    assert_eq!(bodies(&simulation, ids), before, "no site was spawned");
    assert!(!stamped(&simulation, ids), "no ground was taken");

    order_shed(&mut simulation, shed, near, 150.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.build_task, near),
        "a second order onto the plan was accepted"
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        900,
        "only the first order was charged"
    );
}

#[test]
fn an_enemy_plan_does_not_block_and_the_later_arrival_is_refunded() {
    let (mut simulation, ids, shed, near, far) = porters([PLAYER, OTHER], Vec::new());
    order_shed(&mut simulation, shed, near, 220.0, 200.0);
    order_shed(&mut simulation, shed, far, 220.0, 200.0);
    simulation.tick().unwrap();
    assert_eq!(
        purse(&simulation, ids, OTHER),
        900,
        "the enemy's order onto a plan it cannot see was refused"
    );
    for _ in 0..900 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.build_task, far) {
            break;
        }
    }
    assert_eq!(
        simulation
            .state()
            .store(ids.construction)
            .map_or(0, |store| store.len()),
        1,
        "exactly one of the two orders became a site"
    );
    assert_eq!(
        purse(&simulation, ids, OTHER),
        1_000,
        "the refused arrival paid its 100 back in full"
    );
}

#[test]
fn a_plan_is_shown_to_its_side_while_a_builder_is_selected() {
    let shown = |teams: Vec<Option<u8>>, viewer: PlayerId, viewer_selects: bool| {
        let (mut simulation, ids, shed, near, far) = porters([PLAYER, OTHER], teams);
        order_shed(&mut simulation, shed, near, 320.0, 200.0);
        let own = if viewer == PLAYER { near } else { far };
        let selection = if viewer_selects {
            vec![own]
        } else {
            Vec::new()
        };
        select(&mut simulation, viewer, selection);
        simulation.tick().unwrap();
        simulation
            .rules()
            .shown_build_orders(simulation.state(), ids, viewer)
            .len()
    };
    assert_eq!(
        shown(Vec::new(), PLAYER, true),
        1,
        "owner, builder selected"
    );
    assert_eq!(
        shown(Vec::new(), PLAYER, false),
        0,
        "owner, nothing selected"
    );
    assert_eq!(shown(Vec::new(), OTHER, true), 0, "enemy, builder selected");
    assert_eq!(
        shown(vec![Some(0), Some(0)], OTHER, true),
        1,
        "ally, builder selected"
    );
}

#[test]
fn a_builder_on_the_far_side_docks_at_the_near_side_of_the_footprint() {
    let (mut simulation, ids, shed, _near, far) = porters([PLAYER, PLAYER], Vec::new());
    order_shed(&mut simulation, shed, far, 200.0, 200.0);
    simulation.tick().unwrap();
    let path = simulation
        .state()
        .get(ids.path, far)
        .expect("the builder should have a route to the site");
    assert!(
        path.goal.x > 200.0,
        "expected the dock point on the builder's own (east) side of the site at x=200, got {:?}",
        path.goal
    );
}

#[test]
fn re_tasking_a_builder_refunds_its_pending_order() {
    let (mut simulation, ids, shed, _, far) = porters([PLAYER, PLAYER], Vec::new());
    order_shed(&mut simulation, shed, far, 150.0, 200.0);
    simulation.tick().unwrap();
    assert_eq!(purse(&simulation, ids, PLAYER), 900);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 400.0,
                y: 40.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.build_task, far),
        "the order survived the new one"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 1_000, "refunded in full");
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .store(ids.construction)
            .map_or(0, |store| store.len())
            == 0,
        "a cancelled order must never become a site"
    );
}

#[test]
fn a_builder_killed_on_its_way_places_no_site_and_its_order_is_refunded() {
    let (mut simulation, ids, shed, _, far) = porters([PLAYER, PLAYER], Vec::new());
    order_shed(&mut simulation, shed, far, 150.0, 200.0);
    simulation.tick().unwrap();
    assert_eq!(purse(&simulation, ids, PLAYER), 900);
    assert!(simulation.state().contains(ids.build_task, far));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::DebugCommand::Kill {
                target: crate::rules::DebugTarget::All,
            }),
        )
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .store(ids.construction)
            .map_or(0, |store| store.len())
            == 0,
        "a dead builder raised a site"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 1_000, "refunded in full");
}

#[test]
fn a_builder_within_arrival_tolerance_starts_without_walking_the_rest_of_the_way() {
    let (mut simulation, ids, shed, _near, far) = porters([PLAYER, PLAYER], Vec::new());
    order_shed(&mut simulation, shed, far, 300.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.path, far),
        "expected the builder within tolerance to have already cut its path short"
    );
}

#[test]
fn a_builder_faces_the_site_before_building_starts() {
    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let shed_kind = kinds.kind("Shed").unwrap();
    let spawn = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(300.0, 200.0),
            Facing::from_radians(std::f32::consts::PI),
            PLAYER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        61,
        RotwkRules::new(passability, vec![spawn])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 1_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    order_shed(&mut simulation, shed_kind, porter, 380.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .store(ids.construction)
            .is_none_or(|store| store.entities().is_empty()),
        "expected the builder to still be turning to face the site, not already building"
    );
    for _ in 0..20 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .store(ids.construction)
            .is_some_and(|store| !store.entities().is_empty())
        {
            return;
        }
    }
    panic!("the builder never started building once it had turned to face the site");
}

#[test]
fn allow_disallow_one_building_refuses_then_permits_construction() {
    use mint_sage_map::{
        Condition, OrCondition, Parameter, Script, ScriptAction, ScriptNode, ScriptTree,
    };

    fn string_param(value: &str) -> Parameter {
        Parameter::Value {
            param_type: 0,
            int_value: 0,
            real_value: 0.0,
            string_value: value.to_owned(),
        }
    }
    fn int_param(value: i32) -> Parameter {
        Parameter::Value {
            param_type: 0,
            int_value: value,
            real_value: 0.0,
            string_value: String::new(),
        }
    }
    fn one_shot(name: &str, action: &str, params: Vec<Parameter>) -> Script {
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
                    name: "CONDITION_TRUE".to_owned(),
                    params: Vec::new(),
                }],
            }],
            actions: vec![ScriptAction {
                action_type: 0,
                enabled: true,
                name: action.to_owned(),
                params,
            }],
            actions_false: Vec::new(),
            sequence: Default::default(),
        }
    }

    let (data, kinds) = construction_kinds();
    let production = kinds.production(&data);
    let porter_kind = kinds.kind("Porter").unwrap();
    let shed = kinds.kind("Shed").unwrap();
    let porter = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(140.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();

    let disallow = one_shot(
        "Disallow",
        "ALLOW_DISALLOW_ONE_BUILDING",
        vec![
            string_param("<This Player>"),
            string_param("Shed"),
            int_param(0),
        ],
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(disallow)]],
    };
    let engine = crate::rules::map_scripts::MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Player_1")]],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        false,
    );

    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        64,
        RotwkRules::new(passability, vec![porter])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_economy(Vec::new(), 1_000)
            .with_players(1)
            .with_teams(vec![None])
            .with_map_scripts(Some(engine), vec![(0, PLAYER)]),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter_entity = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation.tick().unwrap();
    order_shed(&mut simulation, shed, porter_entity, 150.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .store(ids.construction)
            .is_none_or(|store| store.entities().is_empty()),
        "a disallowed building was still ordered"
    );

    let allow = one_shot(
        "Allow",
        "ALLOW_DISALLOW_ONE_BUILDING",
        vec![
            string_param("<This Player>"),
            string_param("Shed"),
            int_param(1),
        ],
    );
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(allow)]],
    };
    let engine = crate::rules::map_scripts::MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Player_1")]],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        false,
    );
    let porter = kinds
        .spawn(
            &data.combat,
            porter_kind,
            Vec2::new(140.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(160, 60);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        64,
        RotwkRules::new(passability, vec![porter])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production.clone())
            .with_economy(Vec::new(), 1_000)
            .with_players(1)
            .with_teams(vec![None])
            .with_map_scripts(Some(engine), vec![(0, PLAYER)]),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter_entity = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation.tick().unwrap();
    order_shed(&mut simulation, shed, porter_entity, 150.0, 200.0);
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .store(ids.construction)
            .is_some_and(|store| !store.entities().is_empty()),
        "allowing it again still refused the order"
    );
}
