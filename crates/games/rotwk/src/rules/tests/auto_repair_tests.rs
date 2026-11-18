use super::*;

const DATA: &str = "\
Object Shed\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 2\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n    WorkerName = ShedWorkerNoSelect\n    \
  SpawnTimer = 3\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 40\n  GeometryMinorRadius = 40\n  GeometryHeight = 30\nEnd\n\
Object Hut\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 2\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 300\n  End\n  \
  Behavior = GettingBuiltBehavior Tag_Built\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 40\n  GeometryMinorRadius = 40\n  GeometryHeight = 30\nEnd\n\
Object ShedWorkerNoSelect\n  Side = Men\n  KindOf = INFANTRY\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Behavior = WorkerAIUpdate Tag_AI\n    RepairHealthPercentPerSecond = 30%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 12\nEnd\n";

struct Base {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Base {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Shed", "Hut"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    Base {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(b: &Base, name: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let spawn = b
        .kinds
        .spawn(
            &b.data.combat,
            b.kinds.kind(name).unwrap(),
            Vec2::new(300.0, 150.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_players(1)
        .with_combat(b.data.combat.clone())
        .with_construction(Arc::clone(&b.kinds), b.kinds.production(&b.data));
    let simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let structure = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    (simulation, ids, structure)
}

fn hit(simulation: &mut Simulation<RotwkRules>, structure: Entity, percent: u8) {
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![structure],
        },
        RotwkCommand::Debug(DebugCommand::Damage { percent }),
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

fn workers(b: &Base, simulation: &Simulation<RotwkRules>, ids: Ids) -> Vec<Entity> {
    let kind = b.kinds.kind("ShedWorkerNoSelect").unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

#[test]
fn the_worker_comes_once_the_timer_has_run_down_unhit() {
    let b = load();
    let (mut simulation, ids, shed) = start(&b, "Shed");
    let first_worker = |simulation: &mut Simulation<RotwkRules>| {
        while workers(&b, simulation, ids).is_empty() {
            simulation.tick().unwrap();
        }
        simulation.frame().get()
    };
    hit(&mut simulation, shed, 10);
    let hurt = simulation.frame().get();
    let alone = first_worker(&mut simulation);
    let (mut again, _, _) = start(&b, "Shed");
    hit(&mut again, shed, 10);
    for _ in 0..60 {
        again.tick().unwrap();
    }
    hit(&mut again, shed, 10);
    let twice = first_worker(&mut again);
    assert_eq!((alone, twice), (211, 271), "first hit on frame {hurt}");
}

#[test]
fn a_structure_with_no_worker_heals_itself_over_its_build_time() {
    let b = load();
    let (mut simulation, ids, hut) = start(&b, "Hut");
    hit(&mut simulation, hut, 50);
    let mut full_on = None;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if full_on.is_none() && health(&simulation, ids, hut) >= 300.0 {
            full_on = Some(simulation.frame().get());
        }
    }
    assert_eq!(full_on, Some(181));
}

#[test]
fn the_repair_worker_heals_at_its_rate_and_fades_when_done() {
    let b = load();
    let (mut simulation, ids, shed) = start(&b, "Shed");
    hit(&mut simulation, shed, 20);
    let mut steps = Vec::new();
    let mut before = health(&simulation, ids, shed);
    for _ in 0..600 {
        simulation.tick().unwrap();
        let now = health(&simulation, ids, shed);
        if now > before {
            steps.push(now - before);
        }
        before = now;
    }
    let per_frame = 200.0 * 0.3 / 30.0;
    assert!(
        steps.iter().all(|step| (step - per_frame).abs() < 1e-3) && before >= 200.0,
        "steps {steps:?}, health {before}"
    );
    assert!(
        workers(&b, &simulation, ids).is_empty(),
        "its worker is gone once the shed is whole"
    );
}
