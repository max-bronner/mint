use super::fighting_b4_tests::{attack, scene, SHELL, TOGGLES};
use super::*;

fn two_slot_data() -> String {
    let weapon = |name: &str, extra: &str| {
        format!(
            "Weapon {name}\n  AttackRange = 300\n  DelayBetweenShots = 500\n  ClipSize = 3\n  \
             ClipReloadTime = 4000\n  {extra}\n  DamageNugget\n    Damage = 1\n    \
             DamageType = CRUSH\n  End\nEnd\n"
        )
    };
    format!(
        "{TOGGLES}{}{}Object Pair\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Plain\n    \
         Weapon = SECONDARY Plain\n  End\nEnd\n\
         Object Quickpair\n  Side = Men\n{SHELL}  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Plain\n    \
         Weapon = SECONDARY Quick\n  End\nEnd\n",
        weapon("Plain", ""),
        weapon("Quick", "InstantLoadClipOnActivate = Yes"),
    )
}

#[test]
#[ignore = "F124 spawn half held back: calling start_set_reloads at spawn lost evil-m3, cause unproven"]
fn every_set_weapon_starts_reloading_at_creation_unless_it_loads_instantly() {
    let (mut simulation, ids, units) = scene(
        &two_slot_data(),
        &[
            ("Pair", Vec2::new(300.0, 300.0), PLAYER),
            ("Quickpair", Vec2::new(300.0, 360.0), PLAYER),
        ],
    );
    simulation.tick().unwrap();
    let now = simulation.frame().get();
    let other_ready = |unit: Entity| {
        simulation
            .state()
            .get(ids.weapon_slots, unit)
            .and_then(|held| {
                held.states
                    .get(1)
                    .copied()
                    .flatten()
                    .map(|state| state.when_we_can_fire_again().get())
            })
    };
    assert!(other_ready(units[0]).is_some_and(|ready| ready > now));
    assert!(other_ready(units[1]).is_some_and(|ready| ready <= now));
}

const REACH_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Reach\n  AttackRange = 150\n  DelayBetweenShots = 500\n  WeaponBonus = VETERAN RANGE 150%\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
ExperienceLevel ReachLevel1\n  TargetNames = ReachHorde\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel ReachLevel2\n  TargetNames = ReachHorde\n  RequiredExperience = 7\n  Rank = 2\nEnd\n\
CommandSet EmptySet\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_ReachHorde\nEnd\n\
CommandButton Build_ReachHorde\n  Command = UNIT_BUILD\n  Object = ReachHorde\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ReachHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = EmptySet\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Reach\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:20 Y:-20 Position:X:20 Y:20\n  \
  End\nEnd\n";

#[test]
fn a_veteran_hordes_rangefinder_reaches_further() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        REACH_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Camp".to_owned(), "ReachHorde".to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let m = super::experience_tests::Match {
        data,
        kinds: Arc::new(kinds),
    };
    let rules = super::experience_tests::rules(&m, &[])
        .with_horde_rangefinders(m.data.formations.rangefinders().to_vec())
        .with_horde_rangefinder_weapons(m.data.formations.rangefinder_weapons().to_vec());
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "ReachHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let reach = |simulation: &Simulation<RotwkRules>| {
        let world = simulation.state();
        simulation
            .rules()
            .horde_reach(world, ids, horde, world.get(ids.horde, horde).unwrap())
            .unwrap()
    };
    let fresh = reach(&simulation);
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
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 10 })
        .unwrap();
    simulation.tick().unwrap();
    assert!(reach(&simulation) > fresh * 1.4, "{fresh}");
}

fn sweep_data(weapon_extra: &str, points: &[(i32, i32, u8)]) -> String {
    let lines: String = points
        .iter()
        .map(|(x, y, t)| format!("  LinearTarget = X:{x} Y:{y} T:{t}\n"))
        .collect();
    format!(
        "{TOGGLES}Weapon Sweep\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
         RadiusDamageAffects = ENEMIES\n  {weapon_extra}\n{lines}  \
         DamageNugget\n    Damage = 1\n    Radius = 4\n    DamageType = CRUSH\n  End\nEnd\n\
         Object Sweeper\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
         Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
         WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sweep\n  End\n  \
         LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object Post\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
         Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 3\n  GeometryHeight = 16\nEnd\n"
    )
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn sweep_history(data: &str, victim_at: Vec2, posts: [Vec2; 3]) -> Vec<[u32; 3]> {
    let mut spawns = vec![
        ("Sweeper", Vec2::new(300.0, 300.0), PLAYER),
        ("Post", victim_at, OTHER),
    ];
    spawns.extend(posts.iter().map(|at| ("Post", *at, OTHER)));
    let (mut simulation, ids, units) = scene(data, &spawns);
    simulation.tick().unwrap();
    attack(&mut simulation, units[0], units[1]);
    let lost = |simulation: &Simulation<RotwkRules>| {
        [2, 3, 4].map(|at| {
            let health = simulation.state().get(ids.health, units[at]).unwrap().0;
            (health.max() - health.current()).round() as u32
        })
    };
    let mut history: Vec<[u32; 3]> = Vec::new();
    for _ in 0..150 {
        simulation.tick().unwrap();
        let now = lost(&simulation);
        if now != [0, 0, 0] && history.last() != Some(&now) {
            history.push(now);
        }
    }
    history
}

#[test]
fn a_formations_attribute_modifiers_come_and_go_with_the_swap() {
    use super::porcupine_filter_tests as fixture;
    let data = format!(
        "ModifierList Formed\n  Category = FORMATION\n  Modifier = ARMOR 25%\n  \
         Modifier = SPEED 60%\n  Duration = 0\nEnd\n{}",
        fixture::DATA.replace(
            "IsPorcupineFormation = Yes\n",
            "IsPorcupineFormation = Yes\n    AttributeModifiers = Formed\n"
        )
    );
    let (mut simulation, ids) = fixture::simulation(&data, &[]);
    let horde = fixture::horde(&simulation, ids);
    let mut holders = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    holders.push(horde);
    let held = |simulation: &Simulation<RotwkRules>| {
        holders
            .iter()
            .filter(|holder| simulation.state().contains(ids.bonuses, **holder))
            .count()
    };
    assert_eq!(held(&simulation), 0);
    simulation
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert_eq!(held(&simulation), holders.len());
    simulation
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert_eq!(held(&simulation), 0);
}

const EAST_POSTS: [Vec2; 3] = [
    Vec2::new(340.0, 300.0),
    Vec2::new(360.0, 300.0),
    Vec2::new(380.0, 300.0),
];

#[test]
fn a_linear_target_weapon_fires_its_list_a_volley_at_a_time() {
    let data = sweep_data("", &[(40, 0, 0), (60, 0, 1), (80, 0, 1)]);
    let history = sweep_history(&data, Vec2::new(500.0, 300.0), EAST_POSTS);
    assert_eq!(history[..3], [[1, 1, 0], [1, 1, 1], [2, 2, 1]]);
}

#[test]
fn ignore_linear_first_target_passes_over_the_first_point() {
    let data = sweep_data(
        "IgnoreLinearFirstTarget = Yes",
        &[(40, 0, 0), (60, 0, 1), (80, 0, 1)],
    );
    let history = sweep_history(&data, Vec2::new(500.0, 300.0), EAST_POSTS);
    assert_eq!(history[0], [0, 1, 0]);
}

#[test]
fn the_points_turn_with_the_firers_facing() {
    let data = sweep_data("", &[(40, 0, 1)]);
    let posts = [
        Vec2::new(300.0, 340.0),
        Vec2::new(340.0, 300.0),
        Vec2::new(300.0, 260.0),
    ];
    let history = sweep_history(&data, Vec2::new(300.0, 500.0), posts);
    assert_eq!(history[0], [1, 0, 0]);
}

const WALK_BACK_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Weapon Blade\n  AttackRange = 10\n  DelayBetweenShots = 2000\n  FiringDuration = 1000\n  \
  MeleeWeapon = Yes\n  AcceptableAimDelta = 180\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
Object Swordsman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Post\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE BLOCKING_GATE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 5\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n";

fn walk_back_moved(with_gate: bool) -> f32 {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        WALK_BACK_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Swordsman", "Post", "Gate"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: Vec2, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let mut spawns = vec![
        spawn("Post", Vec2::new(520.0, 300.0), OTHER),
        spawn("Swordsman", Vec2::new(420.0, 300.0), PLAYER),
    ];
    let mut footprints = Vec::new();
    if with_gate {
        spawns.push(spawn("Gate", Vec2::new(500.0, 300.0), PLAYER));
        let records = crate::construction::shown_footprint(
            &kinds.get(kinds.kind("Gate").unwrap()).unwrap().record,
            &[],
        );
        footprints.push((2, records));
    }
    #[allow(clippy::reversed_empty_ranges)]
    let map = wall_map(96, 96, 50, 1..0);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_melee_placement(crate::rules::melee_engage::MeleePlacement {
            approach_dist: 200.0,
            ..Default::default()
        })
        .with_castle_siege_stand_back_distance(40.0)
        .with_structure_footprints(footprints)
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    attack(&mut simulation, units[1], units[0]);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    simulation
        .state()
        .get(ids.body, units[1])
        .unwrap()
        .0
        .position
        .x
        - 420.0
}

#[test]
fn a_step_back_through_a_gate_footprint_pulls_the_attacker_back() {
    let open = walk_back_moved(false);
    let gated = walk_back_moved(true);
    assert!(open > 40.0, "{open}");
    assert!(gated < open - 20.0, "{gated} against {open}");
}
