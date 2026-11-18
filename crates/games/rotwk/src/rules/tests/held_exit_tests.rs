use super::*;

const DATA: &str = "\
Weapon ZClaw\n  AttackRange = 60\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object ZTree\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnPointProductionExitUpdate Tag_Exit\n    SpawnPointBoneName = PERCH_\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
    SpawnTemplateName = ZWalker\n    SpawnReplaceDelay = 100000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 30\nEnd\n\
Object ZWalker\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZClaw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 10\n  End\n  VisionRange = 400\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZFoe\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n";

const PERCH: crate::perches::Perch = crate::perches::Perch {
    position: [30.0, 0.0, 0.0],
    facing: Facing::EAST,
};

const TREE_AT: Vec2 = Vec2::new(300.0, 300.0);

fn scenario() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = ["ZTree", "ZFoe"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let tree = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZTree").unwrap(),
            TREE_AT,
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let foe = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZFoe").unwrap(),
            TREE_AT.plus(Vec2::new(70.0, 0.0)),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let spawn_points = kinds
        .iter()
        .map(|entry| {
            if entry.record.name == "ZTree" {
                vec![PERCH]
            } else {
                Vec::new()
            }
        })
        .collect();
    let rules = RotwkRules::new(passability, vec![tree, foe])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_spawn_points(spawn_points);
    let mut simulation = Simulation::new(17, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let before: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let mut walker = None;
    for _ in 0..30 {
        simulation.tick().unwrap();
        if let Some(new) = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .find(|entity| !before.contains(entity))
        {
            walker = Some(new);
            break;
        }
    }
    (simulation, ids, walker.expect("the tree made its walker"))
}

#[test]
fn a_held_walker_refuses_a_move_order_even_though_its_locomotor_can_move() {
    let (mut simulation, ids, walker) = scenario();
    let before = position(&simulation, ids, walker);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![walker],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        position(&simulation, ids, walker),
        before,
        "a held walker must never move"
    );
    assert!(
        !simulation.state().contains(ids.path, walker),
        "no route should ever be installed for it"
    );
}

#[test]
fn a_held_walker_still_fires_on_a_target_in_range() {
    let (mut simulation, ids, _walker) = scenario();
    let target = simulation
        .state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .find(|(_, owner)| owner.0 == OTHER)
        .map(|(entity, _)| entity)
        .unwrap();
    let before = health_of(&simulation, ids, target).unwrap();
    let mut dropped = false;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if health_of(&simulation, ids, target).unwrap_or(before) < before {
            dropped = true;
            break;
        }
    }
    assert!(
        dropped,
        "a held walker with a target in range must still fire"
    );
}
