use super::super::guard::{GuardDecision, GuardState};
use super::*;

pub(super) const DATA: &str = "\
AIData\n  GuardChaseUnitsDuration = 2000\n  GuardEnemyScanRate = 500\n  \
  GuardEnemyReturnScanRate = 1000\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon OneArrow\n  AttackRange = 600\n  DelayBetweenShots = 500\n  ClipSize = 1\n  \
  ClipReloadTime = 999999\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
ModifierList StanceBlind\n  Category = LEADERSHIP\n  Modifier = VISION -90%\nEnd\n\
StanceTemplate PikeTemplate\n  Stance HoldGround\n    AttributeModifier StanceBlind\n  End\n  \
  Stance Porcupine\n    AttributeModifier StanceBlind\n  End\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Archer\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 700\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY OneArrow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = PikeTemplate\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    RanksToReleaseWhenAttacking = 1\n    \
    ThisFormationIsTheMainFormation = Yes\n    AlternateFormation = PorcupineHorde\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
ChildObject PorcupineHorde PikeHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    IsPorcupineFormation = Yes\n    \
    AlternateFormation = PikeHorde\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PikeHorde\n  2 = Build_PorcupineHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_PikeHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n\
CommandButton Build_PorcupineHorde\n  Command = UNIT_BUILD\n  Object = PorcupineHorde\nEnd\n";

const HOME: Vec2 = Vec2::new(300.0, 300.0);

fn simulation(enemies: &[(&str, Vec2)]) -> (Simulation<RotwkRules>, Ids) {
    simulation_of("PikeHorde", enemies)
}

fn simulation_of(template: &str, enemies: &[(&str, Vec2)]) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Archer", "Quarry", "PikeHorde", "PorcupineHorde"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = enemies
        .iter()
        .map(|(template, offset)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(template).unwrap(),
                    Vec2::new(HOME.x + offset.x, HOME.y + offset.y),
                    Facing::EAST,
                    OTHER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == template)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: HOME.x,
                y: HOME.y,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = horde(&simulation, ids);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids)
}

fn horde(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation.state().store(ids.horde).unwrap().entities()[0]
}

fn set_stance(simulation: &mut Simulation<RotwkRules>, stance: mint_sage_ini::Stance) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetStance {
                stance: stance.index(),
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

fn state(simulation: &Simulation<RotwkRules>, ids: Ids) -> Option<GuardState> {
    let horde = horde(simulation, ids);
    simulation
        .state()
        .get(ids.guard, horde)
        .map(|guard| guard.state)
}

fn from_home(simulation: &Simulation<RotwkRules>, ids: Ids) -> f32 {
    let at = simulation
        .state()
        .get(ids.body, horde(simulation, ids))
        .unwrap()
        .0
        .position;
    let (dx, dy) = (at.x - HOME.x, at.y - HOME.y);
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn a_hold_ground_horde_chases_its_attacker_for_the_chase_duration_and_returns() {
    let (mut simulation, ids) = simulation(&[("Archer", Vec2::new(0.0, 250.0))]);
    set_stance(&mut simulation, mint_sage_ini::Stance::HoldGround);
    let mut chase: Option<(u32, Option<u32>)> = None;
    let mut farthest = 0.0f32;
    for frame in 0..600 {
        simulation.tick().unwrap();
        let now = state(&simulation, ids);
        farthest = farthest.max(from_home(&simulation, ids));
        match (chase, now) {
            (None, Some(GuardState::AttackAggressor)) => chase = Some((frame, None)),
            (Some((start, None)), Some(GuardState::Return)) => chase = Some((start, Some(frame))),
            _ => {}
        }
    }
    let (start, end) = chase.expect("the horde never answered the arrow");
    let lasted = end.expect("the chase never ended") - start;
    assert!((58..=62).contains(&lasted), "chased {lasted} frames");
    assert!(
        farthest > 60.0,
        "the horde never left its point: {farthest}"
    );
    assert!(
        from_home(&simulation, ids) < 15.0,
        "not back: {}",
        from_home(&simulation, ids)
    );
    assert_eq!(state(&simulation, ids), Some(GuardState::Idle));
}

#[test]
fn attacked_me_is_polled_only_on_a_retail_frame_tick() {
    let (mut simulation, ids) = simulation(&[("Archer", Vec2::new(0.0, 250.0))]);
    set_stance(&mut simulation, mint_sage_ini::Stance::HoldGround);
    let horde = horde(&simulation, ids);
    let frame_ticks = u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
    let mut hit_at = None;
    let mut transition_at = None;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if hit_at.is_none() {
            hit_at = simulation
                .state()
                .get(ids.last_attacker, horde)
                .map(|last| last.at.get());
        }
        if transition_at.is_none() && state(&simulation, ids) == Some(GuardState::AttackAggressor) {
            transition_at = Some(simulation.frame().get() - 1);
        }
        if transition_at.is_some() {
            break;
        }
    }
    let hit_at = hit_at.expect("the horde was never hit");
    let transition_at = transition_at.expect("the horde never answered the arrow");
    assert_eq!(
        transition_at % frame_ticks,
        0,
        "entered ATTACK_AGGRESSOR off a retail frame boundary at tick {transition_at}"
    );
    assert!(
        transition_at >= hit_at && transition_at - hit_at < frame_ticks,
        "hit at {hit_at}, answered at {transition_at} - more than one frame's delay"
    );
}

#[test]
fn a_battle_horde_takes_the_next_victim_when_its_first_dies() {
    let (mut simulation, ids) = simulation(&[
        ("Quarry", Vec2::new(40.0, 0.0)),
        ("Quarry", Vec2::new(70.0, 0.0)),
    ]);
    let horde = horde(&simulation, ids);
    let mut victims: Vec<Entity> = Vec::new();
    let mut returned_between = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        let held = simulation
            .state()
            .get(ids.attack_order, horde)
            .filter(|order| order.guard)
            .map(|order| order.target);
        if let Some(target) = held {
            if victims.last() != Some(&target) {
                victims.push(target);
            }
        }
        if victims.len() == 1
            && matches!(
                state(&simulation, ids),
                Some(GuardState::Return | GuardState::Idle)
            )
        {
            returned_between = true;
        }
        if victims.len() >= 2 {
            break;
        }
    }
    assert_eq!(victims.len(), 2, "victims {victims:?}");
    assert!(
        !returned_between,
        "the horde went back to its point between victims"
    );
}

#[test]
fn an_order_breaks_porcupine_and_the_horde_guards_where_it_ends() {
    let (mut simulation, ids) = simulation(&[]);
    let horde = horde(&simulation, ids);
    let holds_porcupine = |simulation: &Simulation<RotwkRules>, member: Entity| {
        let wiring = simulation.rules().modules.as_ref().unwrap().state;
        simulation.state().get(wiring, member).is_some_and(|state| {
            state
                .model_conditions
                .iter()
                .any(|name| name.to_string() == "PORCUPINE")
        })
    };
    simulation
        .submit(PLAYER, RotwkCommand::ToggleHordeFormation)
        .unwrap();
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Porcupine),
        "the toggle must have taken before the order that breaks it"
    );
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert!(
        members
            .iter()
            .all(|member| holds_porcupine(&simulation, *member)),
        "the fixture's premise: every member takes PORCUPINE once arrived"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 450.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..400 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.rules().stance_of(simulation.state(), ids, horde),
        Some(mint_sage_ini::Stance::Battle)
    );
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert!(
        members
            .iter()
            .all(|member| !holds_porcupine(&simulation, *member)),
        "PORCUPINE must clear from every member on the switch back"
    );
    let guard = simulation.state().get(ids.guard, horde).copied().unwrap();
    assert!(
        (guard.point.x - 450.0).abs() < 20.0 && (guard.point.y - 300.0).abs() < 20.0,
        "guards {:?}",
        guard.point
    );
}

#[test]
fn the_guard_machine_hashes_the_same_twice() {
    let run = || {
        let (mut simulation, _) = simulation(&[("Archer", Vec2::new(0.0, 250.0))]);
        set_stance(&mut simulation, mint_sage_ini::Stance::HoldGround);
        let mut hashes = Vec::new();
        for _ in 0..200 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn porcupine_members_face_outward_and_take_the_condition_on_arrival() {
    let facings = |template: &str| {
        let (mut simulation, ids) = simulation_of(template, &[]);
        for _ in 0..90 {
            simulation.tick().unwrap();
        }
        let horde = horde(&simulation, ids);
        let centre = simulation.state().get(ids.body, horde).unwrap().0.position;
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .clone();
        let wiring = simulation.rules().modules.as_ref().unwrap().state;
        members
            .into_iter()
            .map(|member| {
                let body = simulation.state().get(ids.body, member).unwrap().0;
                let out = Facing::from_vector(body.position.minus(centre)).unwrap();
                let held = simulation.state().get(wiring, member).is_some_and(|state| {
                    state
                        .model_conditions
                        .iter()
                        .any(|name| name.to_string() == "PORCUPINE")
                });
                (body.facing.delta_to(out).abs(), held)
            })
            .collect::<Vec<_>>()
    };
    let porcupine = facings("PorcupineHorde");
    assert!(
        porcupine.iter().all(|(off, held)| *off < 0.05 && *held),
        "{porcupine:?}"
    );
    let plain = facings("PikeHorde");
    assert!(
        plain.iter().any(|(off, held)| *off > 0.5 && !*held),
        "{plain:?}"
    );
}

#[test]
fn the_guard_decision_is_a_pure_read_that_an_order_changes() {
    let settled = || {
        let (mut simulation, ids) = simulation(&[]);
        for _ in 0..30 {
            simulation.tick().unwrap();
        }
        (simulation, ids)
    };
    let (mut first, ids) = settled();
    let (second, _) = settled();
    let decide = |simulation: &Simulation<RotwkRules>| {
        let horde = horde(simulation, ids);
        simulation
            .rules()
            .decide_guard(simulation.state(), ids, horde)
    };
    let decided = decide(&first);
    assert_eq!(
        decided,
        GuardDecision::Step,
        "the fixture's premise: a guarding horde"
    );
    assert_eq!(decide(&first), decided);
    assert_eq!(decide(&second), decided);
    first
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 450.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    first.tick().unwrap();
    assert_eq!(decide(&first), GuardDecision::Pass);
}
