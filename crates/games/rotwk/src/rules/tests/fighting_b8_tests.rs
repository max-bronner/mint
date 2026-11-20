use super::*;
use crate::melee::MeleePhase;

const CHARGE_MODULE: &str = "Behavior = ModelConditionUpgrade Tag_Charge\n    \
    TriggeredBy = Upgrade_Charge\n    AddConditionFlags = CHARGING\n  End\n  ";

const PLAIN_MODULE: &str = "";

fn data(module: &str) -> String {
    format!(
        "\
Upgrade Upgrade_Charge\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Maul\n  AttackRange = 12\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  VisionRange = 100\n  \
  CrusherLevel = 1\n  CrushOnlyWhileCharging = Yes\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Maul\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RiderHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  \
  CrusherLevel = 1\n  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  \
  CommandSet = RushSet\n  {module}\
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Maul\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Rider Position:X:10 Y:-10 Position:X:10 Y:10\n  End\nEnd\n\
Object Foe\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Behavior = SquishCollide Tag_Squish\n  End\n  Behavior = DestroyDie Tag_Die\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RiderHorde\nEnd\n\
CommandButton Build_RiderHorde\n  Command = UNIT_BUILD\n  Object = RiderHorde\nEnd\n\
CommandSet RushSet\n  1 = Buy_Charge\nEnd\n\
CommandButton Buy_Charge\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Charge\nEnd\n"
    )
}

fn horde_scene(module: &str, foes: &[Vec2]) -> (Simulation<RotwkRules>, Ids, Entity, Vec<Entity>) {
    horde_scene_owned(module, foes, OTHER)
}

fn horde_scene_owned(
    module: &str,
    foes: &[Vec2],
    foe_owner: PlayerId,
) -> (Simulation<RotwkRules>, Ids, Entity, Vec<Entity>) {
    let text = data(module);
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = vec!["RiderHorde".to_owned(), "Foe".to_owned(), "Camp".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let spawned: Vec<_> = foes
        .iter()
        .map(|at| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Foe").unwrap(),
                    *at,
                    Facing::EAST,
                    foe_owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawned)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_melee_hordes(data.formations.melee().to_vec())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec!["Creeps".to_owned(), "Bystander".to_owned()],
            vec![
                crate::alliance::MapRelations {
                    universal_hostile: true,
                    ..crate::alliance::MapRelations::default()
                },
                crate::alliance::MapRelations::default(),
            ],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RiderHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 100.0,
                y: 100.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let foes = simulation
        .state()
        .store(ids.health)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .filter(|entity| simulation.state().get(ids.owner, *entity).map(|o| o.0) == Some(foe_owner))
        .collect();
    (simulation, ids, horde, foes)
}

fn select_member(simulation: &mut Simulation<RotwkRules>, ids: Ids, horde: Entity) {
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![member],
            },
        )
        .unwrap();
}

#[test]
fn a_resting_crusher_horde_runs_through_its_victim() {
    let (mut simulation, ids, horde, foes) = horde_scene(PLAIN_MODULE, &[Vec2::new(300.0, 100.0)]);
    select_member(&mut simulation, ids, horde);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: foes[0] })
        .unwrap();
    let mut squished = false;
    let mut past = false;
    for _ in 0..SIMULATION_HZ {
        simulation.tick().unwrap();
        let state = simulation.state();
        squished |= state
            .get(ids.horde_melee, horde)
            .is_some_and(|melee| matches!(melee.phase, MeleePhase::Squish { .. }));
        past |= state
            .get(ids.path, horde)
            .is_some_and(|path| path.goal.x > 305.0);
    }
    assert!(squished, "the horde never entered the crush run");
    assert!(past, "the crush run's goal never lay past the victim");
}

#[test]
fn a_crusher_horde_does_not_run_through_a_neutral_victim() {
    let entered = |owner| {
        let (mut simulation, ids, horde, foes) =
            horde_scene_owned(PLAIN_MODULE, &[Vec2::new(300.0, 100.0)], owner);
        select_member(&mut simulation, ids, horde);
        simulation
            .submit(PLAYER, RotwkCommand::ForceAttackObject { target: foes[0] })
            .unwrap();
        let mut squished = false;
        let mut melee = false;
        for _ in 0..SIMULATION_HZ {
            simulation.tick().unwrap();
            let held = simulation.state().get(ids.horde_melee, horde).copied();
            melee |= held.is_some();
            squished |= held.is_some_and(|m| matches!(m.phase, MeleePhase::Squish { .. }));
        }
        assert!(melee, "the horde never took the attack");
        squished
    };
    assert!(
        entered(OTHER),
        "the enemy control never entered the crush run"
    );
    assert!(!entered(PlayerId::new(3)));
}

#[test]
fn a_slow_crusher_horde_beside_its_goal_fights_instead_of_running() {
    let squished = |foe_x: f32| {
        let (mut simulation, ids, horde, foes) =
            horde_scene(PLAIN_MODULE, &[Vec2::new(foe_x, 100.0)]);
        select_member(&mut simulation, ids, horde);
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: foes[0] })
            .unwrap();
        simulation.tick().unwrap();
        simulation.tick().unwrap();
        simulation
            .state()
            .get(ids.horde_melee, horde)
            .is_some_and(|melee| matches!(melee.phase, MeleePhase::Squish { .. }))
    };
    assert!(squished(250.0), "the far control never ran");
    assert!(!squished(114.0));
}

#[test]
fn a_fast_crusher_horde_whose_goal_dies_runs_at_the_next_crushable() {
    let (mut simulation, ids, horde, foes) = horde_scene(
        PLAIN_MODULE,
        &[Vec2::new(300.0, 100.0), Vec2::new(115.0, 125.0)],
    );
    select_member(&mut simulation, ids, horde);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: foes[0] })
        .unwrap();
    for _ in 0..SIMULATION_HZ / 2 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_melee, horde)
            .map(|m| m.goal),
        Some(foes[0])
    );
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![foes[0]],
            },
        )
        .unwrap();
    simulation.submit(OTHER, RotwkCommand::DebugKill).unwrap();
    for _ in 0..SIMULATION_HZ / 2 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_melee, horde)
            .map(|m| m.goal),
        Some(foes[1])
    );
}

#[test]
fn a_member_of_a_charging_horde_crushes() {
    let crushed = |module: &str| {
        let (mut simulation, ids, horde, foes) =
            horde_scene(module, &[Vec2::new(200.0, 90.0), Vec2::new(200.0, 110.0)]);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![horde],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
            .unwrap();
        simulation.tick().unwrap();
        select_member(&mut simulation, ids, horde);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 320.0,
                    y: 100.0,
                    queue: false,
                },
            )
            .unwrap();
        for _ in 0..8 * SIMULATION_HZ {
            simulation.tick().unwrap();
        }
        foes[..2]
            .iter()
            .filter(|foe| {
                simulation
                    .state()
                    .get(ids.health, **foe)
                    .is_none_or(|health| health.0.current() <= 0.0)
            })
            .count()
    };
    assert!(
        crushed(CHARGE_MODULE) > 0,
        "a charging horde crushed nothing"
    );
    assert_eq!(crushed(PLAIN_MODULE), 0, "an uncharged horde crushed");
}

const SIGHT_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE {KIND}\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Target\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 20\nEnd\n";

fn damage_over_ridge(kind: &str, ridge: u16) -> f32 {
    let text = SIGHT_DATA.replace("{KIND}", kind);
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = vec!["Archer".to_owned(), "Target".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 300.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawned = vec![
        spawn("Archer", 300.0, PLAYER),
        spawn("Target", 400.0, OTHER),
    ];
    let mut map = flat_map(96, 96);
    for row in 0..96usize {
        for column in 34..37usize {
            map.samples[row * 96 + column] = ridge;
        }
    }
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawned)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    super::fighting_b4_tests::attack(&mut simulation, units[0], units[1]);
    for _ in 0..4 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    100_000.0
        - simulation
            .state()
            .get(ids.health, units[1])
            .unwrap()
            .0
            .current()
}

#[test]
fn a_ridge_above_the_line_blocks_fire_and_one_within_the_allowance_does_not() {
    assert_eq!(damage_over_ridge("ATTACK_NEEDS_LINE_OF_SIGHT", 768), 0.0);
    assert!(damage_over_ridge("ATTACK_NEEDS_LINE_OF_SIGHT", 266) > 0.0);
    assert_eq!(damage_over_ridge("CAN_SHOOT_OVER_WALLS", 768), 0.0);
}
