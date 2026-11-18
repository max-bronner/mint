use super::*;

const DATA: &str = "\
Weapon Sword\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Raider\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    SpecialContactPoints = Repair\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Shed\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  BuildCost = 100\n  BuildTime = 2\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\n  \
  GeometryContactPoint = X:40 Y:0 Z:0\n  \
  GeometryContactPoint = X:-40 Y:0 Z:0\n  \
  GeometryContactPoint = X:0 Y:40 Z:0 Repair\nEnd\n\
CommandSet PorterSet\n  1 = Build_Shed\nEnd\n\
CommandButton Build_Shed\n  Command = DOZER_CONSTRUCT\n  Object = Shed\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Raider", "Porter", "Shed"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn start(
    m: &Match,
    spawns: &[(&str, f32, f32, PlayerId)],
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let production = m.kinds.production(&m.data);
    let spawns: Vec<Spawn> = spawns
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            m.kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 5_000)
        .with_menus(production.menus.clone());
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let mut entities: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    entities.sort_unstable_by_key(|entity| entity.index());
    (simulation, ids, entities)
}

fn claimed_point(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<u16> {
    simulation
        .state()
        .get(ids.contact_claim, entity)
        .map(|claim| claim.point)
}

fn raid() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let m = load();
    let (mut simulation, ids, entities) = start(
        &m,
        &[
            ("Shed", 300.0, 300.0, PLAYER),
            ("Raider", 300.0, 420.0, OTHER),
            ("Raider", 310.0, 420.0, OTHER),
            ("Raider", 290.0, 420.0, OTHER),
        ],
    );
    let shed = entities[0];
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: entities[1..].to_vec(),
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: shed })
        .unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    (simulation, ids, entities)
}

#[test]
fn two_points_take_two_attackers_and_a_third_falls_back_to_the_geometry() {
    let (simulation, ids, entities) = raid();
    let shed = entities[0];
    let points: Vec<Option<u16>> = entities[1..]
        .iter()
        .map(|raider| claimed_point(&simulation, ids, *raider))
        .collect();
    assert_eq!(points, [Some(0), Some(1), None]);
    let engaged = entities[1..]
        .iter()
        .filter(|raider| {
            simulation
                .state()
                .get(ids.armed, **raider)
                .and_then(|armed| armed.target)
                == Some(shed)
        })
        .count();
    assert_eq!(engaged, 3, "every raider swings at the shed");
    let at_point_0 = position(&simulation, ids, entities[1]);
    let (dx, dy) = (at_point_0.x - 340.0, at_point_0.y - 300.0);
    assert!(
        dx * dx + dy * dy <= 25.0 * 25.0,
        "the first raider stands within its reach of point 0, at {at_point_0:?}"
    );
}

#[test]
fn a_point_frees_when_its_holder_dies() {
    let (mut simulation, ids, entities) = raid();
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[1]],
            },
        )
        .unwrap();
    simulation.submit(OTHER, RotwkCommand::DebugKill).unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_eq!(claimed_point(&simulation, ids, entities[3]), Some(0));
}

#[test]
fn a_repair_builder_walks_to_the_repair_point() {
    let m = load();
    let (mut simulation, ids, entities) = start(&m, &[("Porter", 450.0, 330.0, PLAYER)]);
    let porter = entities[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: m.kinds.kind("Shed").unwrap().0,
                x: 300.0,
                y: 300.0,
                angle: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let goal = simulation.state().get(ids.path, porter).unwrap().goal;
    let (dx, dy) = (goal.x - 300.0, goal.y - 340.0);
    assert!(
        dx * dx + dy * dy <= 10.0 * 10.0,
        "the porter's goal {goal:?} is the Repair point (300, 340)"
    );
}
