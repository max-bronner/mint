use super::group_tests::{block_formation_with, melee_pair_of};
use super::*;
use crate::melee::{MeleeKind, SwarmState};
use mint_sage_ini::MeleeBehaviorKind;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 12\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
StanceTemplate DrillStances\n  Stance Aggressive\n    MeleeBehavior = Amoeba\n    End\n  End\n  \
  Stance HoldGround\n    MeleeBehavior = HoldGround\n    End\n  End\nEnd\n\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE MELEE_HORDE\n  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10\n  End\nEnd\n\
ChildObject OwnHorde PlainHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    MeleeBehavior = Amoeba\n    End\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10\n  End\nEnd\n\
ChildObject LeaderHorde PlainHorde\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    MeleeBehavior = WaitForLeader\n    End\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10\n  End\nEnd\n\
ChildObject DrilledHorde OwnHorde\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = DrillStances\n  End\nEnd\n\
ChildObject DrilledPlainHorde PlainHorde\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = DrillStances\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PlainHorde\n  2 = Build_OwnHorde\n  3 = Build_LeaderHorde\n  4 = Build_DrilledHorde\n  5 = Build_DrilledPlainHorde\nEnd\n\
CommandButton Build_PlainHorde\n  Command = UNIT_BUILD\n  Object = PlainHorde\nEnd\n\
CommandButton Build_OwnHorde\n  Command = UNIT_BUILD\n  Object = OwnHorde\nEnd\n\
CommandButton Build_LeaderHorde\n  Command = UNIT_BUILD\n  Object = LeaderHorde\nEnd\n\
CommandButton Build_DrilledHorde\n  Command = UNIT_BUILD\n  Object = DrilledHorde\nEnd\n\
CommandButton Build_DrilledPlainHorde\n  Command = UNIT_BUILD\n  Object = DrilledPlainHorde\nEnd\n";

const TEMPLATES: [&str; 5] = [
    "PlainHorde",
    "OwnHorde",
    "LeaderHorde",
    "DrilledHorde",
    "DrilledPlainHorde",
];

fn hordes_of_each_template() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let mut seeds: Vec<String> = TEMPLATES.iter().map(|name| (*name).to_owned()).collect();
    seeds.push("Camp".to_owned());
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_stances(Arc::clone(&data.stances))
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let catalogue = simulation.rules().buildable_catalogue();
    let recipes: Vec<u16> = TEMPLATES
        .iter()
        .map(|name| {
            catalogue
                .iter()
                .find(|entry| entry.label == *name)
                .unwrap()
                .index
        })
        .collect();
    for (at, recipe) in recipes.into_iter().enumerate() {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::DebugSpawnHorde {
                    recipe,
                    x: 100.0 + 60.0 * at as f32,
                    y: 100.0,
                    facing: 0.0,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    }
    let hordes = simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .to_vec();
    assert_eq!(hordes.len(), TEMPLATES.len());
    (simulation, ids, hordes)
}

fn behaviour(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> MeleeKind {
    let table = simulation.state().get(ids.horde, horde).unwrap();
    simulation
        .rules()
        .behaviour_of(simulation.state(), ids, horde, table)
        .0
}

fn stance(simulation: &mut Simulation<RotwkRules>, horde: Entity, stance: mint_sage_ini::Stance) {
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
        .submit(
            PLAYER,
            RotwkCommand::SetStance {
                stance: stance.index(),
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

#[test]
fn the_stance_picks_the_behaviour_then_the_object_then_swarm() {
    let (mut simulation, ids, hordes) = hordes_of_each_template();
    let [plain, own, leader, drilled, drilled_plain] = hordes[..] else {
        panic!("one horde of each template");
    };
    assert_eq!(behaviour(&simulation, ids, plain), MeleeKind::Swarm);
    assert_eq!(behaviour(&simulation, ids, own), MeleeKind::Amoeba);
    assert_eq!(behaviour(&simulation, ids, leader), MeleeKind::Formation);
    assert_eq!(behaviour(&simulation, ids, drilled), MeleeKind::Amoeba);
    assert_eq!(behaviour(&simulation, ids, drilled_plain), MeleeKind::Swarm);

    stance(&mut simulation, drilled, mint_sage_ini::Stance::HoldGround);
    stance(
        &mut simulation,
        drilled_plain,
        mint_sage_ini::Stance::HoldGround,
    );
    assert_eq!(behaviour(&simulation, ids, drilled), MeleeKind::HoldGround);
    assert_eq!(
        behaviour(&simulation, ids, drilled_plain),
        MeleeKind::HoldGround
    );
    stance(
        &mut simulation,
        drilled_plain,
        mint_sage_ini::Stance::Aggressive,
    );
    assert_eq!(
        behaviour(&simulation, ids, drilled_plain),
        MeleeKind::Amoeba
    );
    stance(
        &mut simulation,
        drilled_plain,
        mint_sage_ini::Stance::Battle,
    );
    assert_eq!(behaviour(&simulation, ids, drilled_plain), MeleeKind::Swarm);
}

#[test]
fn swarm_members_hop_at_most_four_rings_and_wait_two_or_four_seconds() {
    let (mut simulation, ids, sides) = melee_pair_of(
        AvoidanceMode::Off,
        [10_000.0; 2],
        [true; 2],
        block_formation_with(Some(MeleeBehaviorKind::Swarm), vec![0]),
    );
    let all: Vec<Entity> = sides.iter().flatten().copied().collect();
    let mut hops = 0u32;
    let mut longest = 0.0f32;
    let mut waits: Vec<u64> = Vec::new();
    let mut fought = false;
    let mut seen: BTreeMap<Entity, (Vec2, Option<mint_sim::Frame>)> = BTreeMap::new();
    for _ in 0..900 {
        simulation.tick().unwrap();
        let now = simulation.frame();
        for member in &all {
            let Some(record) = simulation.state().get(ids.member_melee, *member).copied() else {
                continue;
            };
            let at = simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position;
            let before = seen.insert(*member, (record.goal, record.wait_until));
            if record.state == SwarmState::Moving && before.map(|b| b.0) != Some(record.goal) {
                hops += 1;
                let hop = record.goal.minus(at);
                longest = longest.max(hop.x.abs().max(hop.y.abs()));
            }
            if let Some(until) = record.wait_until {
                if before.and_then(|b| b.1) != Some(until) {
                    waits.push(until.get() - now.get());
                }
            }
        }
        fought |= engaged(&simulation, ids, &all) > 0;
    }
    assert!(fought, "the armies never fought");
    assert!(hops > 10, "only {hops} hops");
    assert!(longest <= 45.0, "a hop of {longest:.1} units");
    assert!(
        waits
            .iter()
            .all(|wait| wait.abs_diff(60) <= 1 || wait.abs_diff(120) <= 1),
        "waits in ticks: {waits:?}"
    );
    assert!(
        waits.iter().any(|wait| wait.abs_diff(120) <= 1),
        "no member ever failed a search"
    );
}

#[test]
fn hold_ground_members_never_step_once_the_fight_runs() {
    let (mut simulation, ids, sides) = melee_pair_of(
        AvoidanceMode::Off,
        [10_000.0; 2],
        [true; 2],
        block_formation_with(Some(MeleeBehaviorKind::HoldGround), vec![0]),
    );
    let all: Vec<Entity> = sides.iter().flatten().copied().collect();
    let settle = 60u32;
    let mut fought: BTreeMap<Entity, u32> = BTreeMap::new();
    let mut to_slot: BTreeMap<Entity, f32> = BTreeMap::new();
    let mut strayed = 0.0f32;
    let mut swinging: BTreeMap<Entity, Vec2> = BTreeMap::new();
    let mut moved = 0.0f32;
    for _ in 0..900 {
        simulation.tick().unwrap();
        for member in &all {
            let entry = *simulation.state().get(ids.member, *member).unwrap();
            let fighting = simulation
                .state()
                .get(ids.horde_melee, entry.horde)
                .is_some_and(|melee| melee.victim.is_some());
            if !fighting {
                continue;
            }
            let ticks = fought.entry(*member).or_insert(0);
            *ticks += 1;
            if *ticks <= settle {
                continue;
            }
            let (Some(horde), Some(body), Some(slot)) = (
                simulation.state().get(ids.horde, entry.horde),
                simulation.state().get(ids.body, entry.horde),
                entry.slot,
            ) else {
                continue;
            };
            let at = simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position;
            if simulation.state().contains(ids.member_attack, *member) {
                let held = *swinging.entry(*member).or_insert(at);
                moved = moved.max(at.minus(held).length());
                continue;
            }
            swinging.remove(member);
            let poses = crate::horde::slot_poses(horde, &body.0);
            if let Some(pose) = poses.get(usize::from(slot)) {
                let gap = at.minus(*pose).length();
                if let Some(before) = to_slot.insert(*member, gap) {
                    strayed = strayed.max(gap - before);
                }
            }
        }
    }
    assert!(!fought.is_empty(), "no fight ever began");
    assert!(engaged(&simulation, ids, &all) > 0, "nobody swung");
    assert!(
        strayed < 0.01,
        "a member not swinging moved {strayed:.2} away from its slot"
    );
    assert!(moved < 1.0, "a fighter moved {moved:.1} while swinging");
}

#[test]
fn only_the_amoeba_re_centres_a_fighting_horde() {
    let shift = |behaviour: Option<MeleeBehaviorKind>| {
        let (mut simulation, ids, sides) = melee_pair_of(
            AvoidanceMode::Off,
            [10_000.0; 2],
            [true; 2],
            block_formation_with(behaviour, vec![0]),
        );
        let all: Vec<Entity> = sides.iter().flatten().copied().collect();
        let mut held: BTreeMap<Entity, Vec2> = BTreeMap::new();
        let mut shift = 0.0f32;
        for _ in 0..900 {
            simulation.tick().unwrap();
            for member in &all {
                let horde = simulation.state().get(ids.member, *member).unwrap().horde;
                let fighting = simulation
                    .state()
                    .get(ids.horde_melee, horde)
                    .is_some_and(|melee| melee.victim.is_some())
                    && !simulation.state().contains(ids.path, horde);
                if !fighting {
                    held.remove(&horde);
                    continue;
                }
                let at = simulation.state().get(ids.body, horde).unwrap().0.position;
                let first = *held.entry(horde).or_insert(at);
                shift = shift.max(at.minus(first).length());
            }
        }
        shift
    };
    let swarm = shift(None);
    let amoeba = shift(Some(MeleeBehaviorKind::Amoeba));
    assert!(
        swarm < 0.01,
        "a Swarm horde moved {swarm:.2} while fighting"
    );
    assert!(
        amoeba > 1.0,
        "the fixture's Amoeba horde never re-centred: {amoeba:.2}"
    );
}

#[test]
fn only_the_released_ranks_of_a_ranged_horde_fire() {
    let holding = |ranks: Vec<u32>| {
        let (combat, bow, armour) = combat_data();
        let map = flat_map(128, 128);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation = block_formation_with(None, ranks);
        let count = u32::try_from(formation.len()).unwrap();
        let spawn = |x: f32, owner: PlayerId| HordeSpawn {
            formation: 0,
            origin: Vec2::new(x, 300.0),
            facing: Facing::EAST,
            member: Spawn {
                health: Some(10_000.0),
                weapon: Some(bow),
                armour: Some(armour),
                ..Spawn::inert(
                    Vec2::ZERO,
                    Facing::EAST,
                    Shape {
                        kind_of: KindOf::SELECTABLE,
                        radius: 8.0,
                        height: 18.0,
                        geometry_is_small: false,
                    },
                    owner,
                )
            },
            count,
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        };
        let rules = RotwkRules::new(passability, Vec::new())
            .with_combat(combat)
            .with_hordes(
                vec![formation],
                vec![spawn(300.0, PLAYER), spawn(450.0, OTHER)],
            );
        let mut simulation = Simulation::new(3, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let hordes = simulation
            .state()
            .store(ids.horde)
            .unwrap()
            .entities()
            .to_vec();
        let mine = simulation
            .state()
            .get(ids.horde, hordes[0])
            .unwrap()
            .members
            .clone();
        let theirs = simulation
            .state()
            .get(ids.horde, hordes[1])
            .unwrap()
            .members
            .clone();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: mine.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
            .unwrap();
        for _ in 0..120 {
            simulation.tick().unwrap();
        }
        engaged(&simulation, ids, &mine)
    };
    assert_eq!(holding(vec![]), 0, "an empty set released someone");
    assert_eq!(holding(vec![7]), 0, "a rank no slot holds released someone");
    assert!(holding(vec![0]) > 0, "the released rank never fired");
}

#[test]
fn a_ranged_horde_closes_until_its_own_weapon_reaches_so_every_rank_fires() {
    let firing = |rangefinder: Option<f32>| {
        let (combat, bow, armour) = combat_data();
        let map = flat_map(128, 128);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let formation = block_formation_with(None, vec![0, 1, 2, 3]);
        let count = u32::try_from(formation.len()).unwrap();
        let spawn = |x: f32, owner: PlayerId, weapon| HordeSpawn {
            formation: 0,
            origin: Vec2::new(x, 300.0),
            facing: Facing::EAST,
            member: Spawn {
                health: Some(100_000.0),
                weapon,
                armour: Some(armour),
                ..Spawn::inert(
                    Vec2::ZERO,
                    Facing::EAST,
                    Shape {
                        kind_of: KindOf::SELECTABLE,
                        radius: 8.0,
                        height: 18.0,
                        geometry_is_small: false,
                    },
                    owner,
                )
            },
            count,
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        };
        let rules = RotwkRules::new(passability, Vec::new())
            .with_combat(combat)
            .with_hordes(
                vec![formation],
                vec![spawn(200.0, PLAYER, Some(bow)), spawn(900.0, OTHER, None)],
            )
            .with_horde_rangefinders(vec![rangefinder]);
        let mut simulation = Simulation::new(3, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let hordes = simulation
            .state()
            .store(ids.horde)
            .unwrap()
            .entities()
            .to_vec();
        let members = |horde: Entity| {
            simulation
                .state()
                .get(ids.horde, horde)
                .unwrap()
                .members
                .clone()
        };
        let (mine, theirs) = (members(hordes[0]), members(hordes[1]));
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: mine.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: theirs[0] })
            .unwrap();
        let mut most = 0;
        for _ in 0..900 {
            simulation.tick().unwrap();
            most = most.max(engaged(&simulation, ids, &mine));
        }
        (most, mine.len())
    };
    let (fired, of) = firing(Some(80.0));
    assert_eq!(
        fired, of,
        "only {fired} of {of} members fired with the horde's own reach"
    );
    let (fired, of) = firing(None);
    assert!(
        fired < of,
        "the fixture's front rank alone reaches: {fired} of {of}"
    );
}
