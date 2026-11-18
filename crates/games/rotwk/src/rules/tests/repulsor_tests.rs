use super::*;

const DATA: &str = "\
Object Townsperson\n  Side = Men\n  KindOf = INFANTRY SELECTABLE CAN_BE_REPULSED\n  \
  VisionRange = 100\n  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Bystander\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  VisionRange = 100\n  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Scarer\n  Side = Orcs\n  KindOf = INFANTRY SELECTABLE\n  \
  VisionRange = 100\n  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const RADIUS: f32 = 220.0;

struct Scene {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    townsperson: Entity,
    bystander: Entity,
    scarer: Entity,
    bystander_scarer: Entity,
}

fn blocked_map() -> HeightMapData {
    let mut map = flat_map(96, 96);
    for sy in 26..=34u32 {
        for sx in 8..=14u32 {
            if (sx + sy) % 2 == 0 {
                map.samples[(sy * 96 + sx) as usize] = 400;
            }
        }
    }
    map
}

fn scene(map: &HeightMapData) -> Scene {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = ["Townsperson", "Bystander", "Scarer"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("Townsperson", Vec2::new(300.0, 300.0), PLAYER),
        ("Bystander", Vec2::new(300.0, 500.0), PLAYER),
        ("Scarer", Vec2::new(330.0, 300.0), OTHER),
        ("Scarer", Vec2::new(330.0, 500.0), OTHER),
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
    let passability = Passability::build(map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(2);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let find = |kind: &str, nth: usize| {
        let kind = kinds.kind(kind).unwrap();
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(_, k)| **k == kind)
            .nth(nth)
            .unwrap()
            .0
    };
    Scene {
        townsperson: find("Townsperson", 0),
        bystander: find("Bystander", 0),
        scarer: find("Scarer", 0),
        bystander_scarer: find("Scarer", 1),
        simulation,
        ids,
    }
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn status(simulation: &mut Simulation<RotwkRules>, who: Entity, status: &str) {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![who],
        },
        RotwkCommand::Debug(crate::rules::debug::DebugCommand::SetStatus {
            status: status.to_owned(),
        }),
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
}

fn hit(simulation: &mut Simulation<RotwkRules>, who: Entity) {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![who],
        },
        RotwkCommand::Debug(crate::rules::debug::DebugCommand::Damage { percent: 10 }),
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
}

fn is_repulsor(scene: &Scene, who: Entity) -> bool {
    scene
        .simulation
        .rules()
        .has_status_bit(scene.simulation.state(), who, "REPULSOR")
}

fn position(scene: &Scene, who: Entity) -> Vec2 {
    scene
        .simulation
        .state()
        .get(scene.ids.body, who)
        .unwrap()
        .0
        .position
}

#[test]
fn a_hurt_townsperson_is_a_repulsor_for_exactly_two_seconds() {
    let mut scene = scene(&flat_map(96, 96));
    let (townsperson, bystander) = (scene.townsperson, scene.bystander);
    hit(&mut scene.simulation, townsperson);
    run(&mut scene.simulation, 1);
    assert!(is_repulsor(&scene, townsperson), "set by the hit");
    run(&mut scene.simulation, 50);
    assert!(is_repulsor(&scene, townsperson), "still on at 51 ticks");
    run(&mut scene.simulation, 15);
    assert!(!is_repulsor(&scene, townsperson), "cleared by 66 ticks");
    hit(&mut scene.simulation, bystander);
    run(&mut scene.simulation, 1);
    assert!(!is_repulsor(&scene, bystander), "no KindOf, no repulsor");
}

#[test]
fn an_idle_townsperson_flees_a_repulsor_and_a_bystander_does_not() {
    let mut scene = scene(&flat_map(96, 96));
    let (townsperson, bystander, scarer) = (scene.townsperson, scene.bystander, scene.scarer);
    let bystander_start = position(&scene, bystander);
    status(&mut scene.simulation, scarer, "REPULSOR");
    let second = scene.bystander_scarer;
    status(&mut scene.simulation, second, "REPULSOR");
    run(&mut scene.simulation, 20);
    assert!(
        scene
            .simulation
            .state()
            .get(scene.ids.repulsed, townsperson)
            .is_some_and(|state| state.is_fleeing()),
        "the townsperson enters the flee"
    );
    run(&mut scene.simulation, 300);
    let gone = position(&scene, townsperson)
        .minus(position(&scene, scarer))
        .length();
    assert!(
        gone > RADIUS * 0.9,
        "ran {gone} from the repulsor, wanted past {RADIUS}"
    );
    assert_eq!(
        position(&scene, bystander),
        bystander_start,
        "the bystander stood still"
    );
}

#[test]
fn the_flee_goal_is_a_walkable_cell_past_the_radius() {
    let mut scene = scene(&blocked_map());
    let (townsperson, scarer) = (scene.townsperson, scene.scarer);
    status(&mut scene.simulation, scarer, "REPULSOR");
    run(&mut scene.simulation, 20);
    let goal = scene
        .simulation
        .state()
        .get(scene.ids.repulsed, townsperson)
        .unwrap()
        .goal();
    let rules = scene.simulation.rules();
    let cell = rules.passability.cell_at(goal).unwrap();
    assert!(
        mint_movement::Passability::is_passable(
            &rules.passability.grid,
            cell,
            mint_sage_passability::GROUND
        ),
        "goal {goal:?} is not walkable"
    );
    let from = goal.minus(position(&scene, scarer)).length();
    assert!(
        from > RADIUS,
        "goal {from} from the scarer, wanted past {RADIUS}"
    );
}

#[test]
fn a_finished_flee_wanders_in_place_until_an_order_replaces_it() {
    let mut scene = scene(&flat_map(96, 96));
    let (townsperson, scarer) = (scene.townsperson, scene.scarer);
    status(&mut scene.simulation, scarer, "REPULSOR");
    run(&mut scene.simulation, 600);
    let state = scene
        .simulation
        .state()
        .get(scene.ids.repulsed, townsperson)
        .copied()
        .expect("still in a repulsed state");
    assert!(!state.is_fleeing(), "the flee ended in the wander");
    scene
        .simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![townsperson],
            },
        )
        .unwrap();
    scene
        .simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 700.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut scene.simulation, 2);
    assert!(!scene
        .simulation
        .state()
        .contains(scene.ids.repulsed, townsperson));
}

#[test]
fn the_prior_repulsor_survives_an_order_that_ends_the_flee() {
    let mut scene = scene(&flat_map(96, 96));
    let (townsperson, scarer, second) = (scene.townsperson, scene.scarer, scene.bystander_scarer);
    status(&mut scene.simulation, scarer, "REPULSOR");
    run(&mut scene.simulation, 20);
    let pair = |scene: &Scene| {
        scene
            .simulation
            .state()
            .get(scene.ids.repulsor_pair, townsperson)
            .copied()
            .unwrap()
    };
    assert_eq!(pair(&scene).prior(), None);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![townsperson],
        },
        RotwkCommand::MoveTo {
            x: 300.0,
            y: 480.0,
            queue: false,
        },
    ] {
        scene.simulation.submit(PLAYER, command).unwrap();
    }
    run(&mut scene.simulation, 400);
    status(&mut scene.simulation, second, "REPULSOR");
    run(&mut scene.simulation, 20);
    assert_eq!(pair(&scene).prior(), Some(scarer));
}

#[test]
fn a_flee_that_loses_its_path_wanders_in_place_without_walking_on() {
    let mut scene = scene(&flat_map(96, 96));
    let (townsperson, scarer) = (scene.townsperson, scene.scarer);
    status(&mut scene.simulation, scarer, "REPULSOR");
    run(&mut scene.simulation, 12);
    for _ in 0..200 {
        if position(&scene, townsperson)
            .minus(position(&scene, scarer))
            .length()
            >= 110.0
        {
            break;
        }
        run(&mut scene.simulation, 1);
    }
    let state = *scene
        .simulation
        .state()
        .get(scene.ids.repulsed, townsperson)
        .expect("the flee has begun");
    assert!(state.is_fleeing());
    let goal = state.goal();
    let lost_at = position(&scene, townsperson);
    assert!(
        lost_at.minus(goal).length() > 100.0,
        "still far from its goal"
    );
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![townsperson],
        },
        RotwkCommand::Debug(crate::rules::debug::DebugCommand::DropPath),
    ] {
        scene.simulation.submit(PLAYER, command).unwrap();
    }
    run(&mut scene.simulation, 90);
    let state = scene
        .simulation
        .state()
        .get(scene.ids.repulsed, townsperson)
        .expect("a wander replaced the flee");
    assert!(!state.is_fleeing(), "the lost path ended the flee");
    let now = position(&scene, townsperson);
    assert!(
        now.minus(lost_at).length() < 60.0,
        "wandered {} from where the path was lost",
        now.minus(lost_at).length()
    );
    assert!(
        now.minus(goal).length() > 80.0,
        "walked on to the flee goal {goal:?}, at {now:?}"
    );
}
