use super::*;

const SWORD_RANGE: f32 = 11.5;
const SOLDIER_RADIUS: f32 = 8.0;

fn block_formation() -> FormationTable {
    block_formation_with(Some(mint_sage_ini::MeleeBehaviorKind::Amoeba), vec![0])
}

pub(super) fn block_formation_with(
    behaviour: Option<mint_sage_ini::MeleeBehaviorKind>,
    ranks: Vec<u32>,
) -> FormationTable {
    let mut offsets = Vec::new();
    for rank in 0..3i16 {
        for file in -2..=2i16 {
            offsets.push(Vec2::new(
                f32::from(1 - rank) * 20.0,
                f32::from(file) * 20.0,
            ));
        }
    }
    let settings = table_of(&offsets).settings().clone();
    table_of(&offsets).with_settings(mint_sage_formation::HordeSettings {
        melee_behavior: behaviour.map(|kind| mint_sage_ini::MeleeBehaviorDef {
            kind,
            amoeba: (kind == mint_sage_ini::MeleeBehaviorKind::Amoeba)
                .then(mint_sage_ini::AmoebaDef::default),
        }),
        ranks_to_release: ranks,
        ..settings
    })
}

pub(super) fn retail_sword(combat: &mut CombatData) -> mint_sage_combat::WeaponId {
    use mint_sage_combat::{DamageNugget, Nugget, WeaponTemplate};
    let slash = combat.damage_types.intern("SLASH").unwrap();
    combat
        .weapons
        .insert(
            "RetailSword",
            WeaponTemplate {
                attack_range: SWORD_RANGE,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(
                    mint_sim::LogicFrames::from_frames(30),
                ),
                clip_size: 0,
                melee: true,
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 2.0,
                    damage_type: slash,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: mint_sim::LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap()
}

const RANGEFINDER: f32 = 12.0;

fn melee_pair(avoidance: AvoidanceMode) -> (Simulation<RotwkRules>, Ids, [Vec<Entity>; 2]) {
    melee_pair_with(avoidance, [10_000.0; 2], [true; 2])
}

fn melee_pair_with(
    avoidance: AvoidanceMode,
    health: [f32; 2],
    ordered: [bool; 2],
) -> (Simulation<RotwkRules>, Ids, [Vec<Entity>; 2]) {
    melee_pair_of(avoidance, health, ordered, block_formation())
}

pub(super) fn melee_pair_of(
    avoidance: AvoidanceMode,
    health: [f32; 2],
    ordered: [bool; 2],
    formation: FormationTable,
) -> (Simulation<RotwkRules>, Ids, [Vec<Entity>; 2]) {
    let (mut combat, _bow, armour) = combat_data();
    let sword = retail_sword(&mut combat);
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn_of = |origin: Vec2, facing: Facing, owner: PlayerId, health: f32| HordeSpawn {
        formation: 0,
        origin,
        facing,
        member: Spawn {
            health: Some(health),
            weapon: Some(sword),
            armour: Some(armour),
            ..Spawn::inert(
                Vec2::ZERO,
                Facing::EAST,
                Shape {
                    kind_of: KindOf::SELECTABLE,
                    radius: SOLDIER_RADIUS,
                    height: 18.0,
                    geometry_is_small: false,
                },
                owner,
            )
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let west_facing = Facing::from_vector(Vec2::new(-1.0, 0.0)).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(
            vec![formation],
            vec![
                spawn_of(Vec2::new(300.0, 300.0), Facing::EAST, PLAYER, health[0]),
                spawn_of(Vec2::new(500.0, 300.0), west_facing, OTHER, health[1]),
            ],
        )
        .with_melee_hordes(vec![Some(RANGEFINDER)]);
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetMovementModes(Modes {
                avoidance,
                ..Modes::default()
            }),
        )
        .unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    assert_eq!(hordes.len(), 2);
    let members = |h: Entity| {
        simulation
            .state()
            .get(ids.horde, h)
            .map(|horde| horde.members.clone())
            .unwrap_or_default()
    };
    let sides = [members(hordes[0]), members(hordes[1])];
    for (player, own, theirs) in [
        (PLAYER, &sides[0], &sides[1]),
        (OTHER, &sides[1], &sides[0]),
    ]
    .into_iter()
    .zip(ordered)
    .filter_map(|(side, ordered)| ordered.then_some(side))
    {
        simulation
            .submit(
                player,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: own.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(player, RotwkCommand::AttackObject { target: theirs[0] })
            .unwrap();
    }
    (simulation, ids, sides)
}

#[derive(Debug, Default)]
struct MeleeShape {
    contact_ticks: u32,
    closest_opposing: f32,
    farthest_from_centre: f32,
    mean_fighting: f32,
    fighting_at_end: usize,
}

fn centroid(simulation: &Simulation<RotwkRules>, ids: Ids, members: &[Entity]) -> Vec2 {
    let points: Vec<Vec2> = members
        .iter()
        .filter_map(|m| simulation.state().get(ids.body, *m).map(|b| b.0.position))
        .collect();
    let sum = points.iter().fold(Vec2::ZERO, |acc, p| acc.plus(*p));
    sum.scale(1.0 / points.len().max(1) as f32)
}

fn measure_melee(avoidance: AvoidanceMode, ticks: u32) -> (MeleeShape, u64) {
    let (mut simulation, ids, sides) = melee_pair(avoidance);
    let all: Vec<Entity> = sides.iter().flatten().copied().collect();
    let mut shape = MeleeShape {
        closest_opposing: f32::MAX,
        ..MeleeShape::default()
    };
    for _ in 0..ticks {
        simulation.tick().unwrap();
        let fighting = engaged(&simulation, ids, &all);
        if fighting == 0 {
            continue;
        }
        shape.contact_ticks += 1;
        shape.mean_fighting += fighting as f32;
        let at = |m: Entity| simulation.state().get(ids.body, m).map(|b| b.0.position);
        for member in &all {
            let horde = simulation.state().get(ids.member, *member).map(|m| m.horde);
            if let (Some(p), Some(centre)) = (at(*member), horde.and_then(at)) {
                let d = p.minus(centre).length();
                shape.farthest_from_centre = shape.farthest_from_centre.max(d);
            }
        }
        for a in &sides[0] {
            for b in &sides[1] {
                if let (Some(pa), Some(pb)) = (at(*a), at(*b)) {
                    shape.closest_opposing = shape.closest_opposing.min(pa.minus(pb).length());
                }
            }
        }
    }
    shape.mean_fighting /= shape.contact_ticks.max(1) as f32;
    shape.fighting_at_end = engaged(&simulation, ids, &all);
    (shape, simulation.state_hash())
}

#[test]
fn two_melee_hordes_fight_at_their_front_and_keep_around_their_centres() {
    for avoidance in [AvoidanceMode::Off, AvoidanceMode::Separation] {
        let (shape, _) = measure_melee(avoidance, 600);
        println!("melee shape ({avoidance:?}): {shape:?}");
        assert!(
            shape.contact_ticks > 400,
            "the hordes must meet and keep fighting: {shape:?}"
        );
        assert!(
            shape.mean_fighting >= 8.0,
            "more than a corner of each front must fight: {shape:?}"
        );
        let outer = mint_sage_ini::AmoebaDef::default().outer_range;
        assert!(
            shape.farthest_from_centre <= outer + 2.0 * 10.0 * core::f32::consts::SQRT_2,
            "a member leaves its horde: {shape:?}"
        );
        assert!(
            shape.closest_opposing >= SOLDIER_RADIUS,
            "opposing soldiers must not stand inside each other: {shape:?}"
        );
    }
}

fn horde_border_gap(simulation: &Simulation<RotwkRules>, ids: Ids, hordes: [Entity; 2]) -> f32 {
    let state = simulation.state();
    let at = |h: Entity| state.get(ids.body, h).unwrap().0.position;
    let radius = |h: Entity| {
        state
            .get(ids.horde, h)
            .unwrap()
            .offsets()
            .fold(0.0f32, |far, offset| far.max(offset.length()))
            + SOLDIER_RADIUS
    };
    at(hordes[0]).minus(at(hordes[1])).length() - radius(hordes[0]) - radius(hordes[1])
}

fn clock(simulation: &Simulation<RotwkRules>, ids: Ids) -> u64 {
    simulation
        .state()
        .store(ids.clock)
        .and_then(|store| store.iter().next().map(|(_, clock)| clock.0))
        .unwrap()
}

#[test]
fn a_melee_horde_stops_its_approach_when_its_border_reaches_the_enemy_hordes() {
    let (mut simulation, ids, sides) =
        melee_pair_with(AvoidanceMode::Off, [10_000.0; 2], [true, false]);
    let horde_of = |m: Entity, simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.member, m).unwrap().horde
    };
    let hordes = [
        horde_of(sides[0][0], &simulation),
        horde_of(sides[1][0], &simulation),
    ];
    let mut stopped_at = None;
    for _ in 0..400 {
        simulation.tick().unwrap();
        let phase = simulation
            .state()
            .get(ids.horde_melee, hordes[0])
            .map(|melee| melee.phase);
        if !matches!(phase, None | Some(crate::melee::MeleePhase::Approach)) {
            stopped_at = Some(horde_border_gap(&simulation, ids, hordes));
            break;
        }
    }
    let gap = stopped_at.expect("the approach must end");
    let reach = RANGEFINDER - 10.0;
    assert!(
        (reach - 8.0..reach).contains(&gap),
        "the borders stop {gap} apart"
    );
}

#[test]
fn a_fighter_gives_up_a_victim_out_of_reach_after_two_seconds() {
    let (mut simulation, ids, sides) = melee_pair(AvoidanceMode::Off);
    let attacker = simulation
        .state()
        .get(ids.member, sides[0][0])
        .unwrap()
        .horde;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if sides[0]
            .iter()
            .any(|m| simulation.state().contains(ids.member_attack, *m))
        {
            break;
        }
    }
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: sides[1].clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 1000.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let give_up =
        u64::from(mint_sim::LogicFrames::from_engine_milliseconds(crate::melee::GIVE_UP_MS).get());
    let mut given_up = 0;
    let mut reapproached = false;
    let mut held = std::collections::BTreeMap::new();
    for _ in 0..240 {
        simulation.tick().unwrap();
        let now = clock(&simulation, ids);
        for member in &sides[0] {
            let attack = simulation.state().get(ids.member_attack, *member).copied();
            if let (None, Some(before)) = (attack, held.get(member)) {
                let before: &crate::melee::MemberAttack = before;
                if let Some(since) = before.out_of_reach_since {
                    let waited = now - since.get();
                    assert!(waited <= give_up + 1, "given up after {waited}");
                }
                given_up += 1;
            }
            match attack {
                Some(attack) => held.insert(*member, attack),
                None => held.remove(member),
            };
        }
        reapproached |= simulation.state().contains(ids.path, attacker);
    }
    assert!(given_up > 0, "a fighter must give its victim up");
    assert!(reapproached, "the attacker re-approaches its fleeing enemy");
}

#[test]
fn wait_path_ends_the_attack_at_the_sixth_check_without_a_route() {
    use crate::melee::MeleePhase;
    let entry = Frame::new(100);
    let MeleePhase::WaitPath { until, .. } = RotwkRules::wait_path(entry, true) else {
        panic!("not wait-path");
    };
    assert_eq!(until, Frame::new(142));
    let mut phase = RotwkRules::wait_path(entry, false);
    let mut checked = Vec::new();
    loop {
        let MeleePhase::WaitPath { until, .. } = phase else {
            panic!("left wait-path without a route");
        };
        checked.push(until.get());
        assert_eq!(
            RotwkRules::wait_path_step(phase, until, true),
            Some(MeleePhase::Approach)
        );
        match RotwkRules::wait_path_step(phase, until, false) {
            Some(next) => phase = next,
            None => break,
        }
    }
    assert_eq!(checked, vec![100, 142, 184, 226, 268, 310]);
}

#[test]
fn at_the_fights_end_every_member_walks_back_to_its_slot() {
    let (mut simulation, ids, sides) =
        melee_pair_with(AvoidanceMode::Off, [10_000.0, 6.0], [true, false]);
    let horde = simulation
        .state()
        .get(ids.member, sides[0][0])
        .unwrap()
        .horde;
    let mut fought = false;
    for _ in 0..1200 {
        simulation.tick().unwrap();
        let fighting = simulation.state().contains(ids.horde_melee, horde);
        if fought && !fighting {
            break;
        }
        fought |= fighting;
    }
    assert!(fought && !simulation.state().contains(ids.horde_melee, horde));
    let ended = simulation.state().get(ids.body, horde).unwrap().0.position;
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    assert_eq!(
        body.position, ended,
        "the horde stays where the fight left it"
    );
    let table = simulation.state().get(ids.horde, horde).unwrap().clone();
    let poses = crate::horde::slot_poses(&table, &body);
    for member in &sides[0] {
        let slot = simulation
            .state()
            .get(ids.member, *member)
            .unwrap()
            .slot
            .unwrap();
        let now = simulation
            .state()
            .get(ids.body, *member)
            .unwrap()
            .0
            .position;
        assert!(
            now.minus(poses[usize::from(slot)]).length() < 0.5,
            "{member:?} walks back"
        );
    }
}

#[test]
fn a_melee_replays_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, _, _) = melee_pair(AvoidanceMode::Separation);
        (0..300)
            .map(|_| {
                simulation.tick().unwrap();
                simulation.state_hash()
            })
            .collect::<Vec<u64>>()
    };
    assert_eq!(run(), run());
}

fn column_of_hordes() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let (mut combat, _bow, armour) = combat_data();
    let sword = retail_sword(&mut combat);
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = block_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawns = [180.0, 300.0, 420.0]
        .iter()
        .map(|y| HordeSpawn {
            formation: 0,
            origin: Vec2::new(200.0, *y),
            facing: Facing::EAST,
            member: Spawn {
                health: Some(200.0),
                weapon: Some(sword),
                armour: Some(armour),
                ..Spawn::inert(
                    Vec2::ZERO,
                    Facing::EAST,
                    Shape {
                        kind_of: KindOf::SELECTABLE,
                        radius: SOLDIER_RADIUS,
                        height: 18.0,
                        geometry_is_small: false,
                    },
                    PLAYER,
                )
            },
            count,
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        })
        .collect();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(vec![formation], spawns);
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let members: Vec<Entity> = hordes
        .iter()
        .flat_map(|h| {
            simulation
                .state()
                .get(ids.horde, *h)
                .unwrap()
                .members
                .clone()
        })
        .collect();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 800.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    (simulation, ids, hordes)
}

#[test]
fn hordes_moved_together_form_one_blob_and_never_merge() {
    let (mut simulation, ids, hordes) = column_of_hordes();
    for _ in 0..900 {
        simulation.tick().unwrap();
    }
    let placed = blocks(&simulation, ids, &hordes);
    let report: Vec<String> = placed
        .iter()
        .map(|b| format!("({:.0}, {:.0})", b.centre.x, b.centre.y))
        .collect();
    println!("group move: block centres {report:?}");
    let click = Vec2::new(800.0, 300.0);
    for (i, a) in placed.iter().enumerate() {
        assert!(
            a.centre.minus(click).length() < 180.0,
            "a horde stands far from the click: {report:?}"
        );
        for b in placed.iter().skip(i + 1) {
            let depth = a.penetration(b);
            println!("block overlap {depth:.1}");
            assert!(depth <= SOLDIER_RADIUS, "hordes merge: {report:?}");
            assert!(
                squares_apart(a, b),
                "claim squares overlap - retail spacing lost: {report:?}"
            );
        }
    }
}

#[test]
fn claim_squares_follow_the_cell_rule() {
    assert_eq!(RotwkRules::claim_half_side(110.0, None, true, 10.0), 45.0);
    assert_eq!(RotwkRules::claim_half_side(110.0, None, false, 10.0), 25.0);
    assert_eq!(RotwkRules::claim_half_side(16.0, None, false, 10.0), 10.0);
    assert_eq!(RotwkRules::claim_half_side(8.0, None, false, 10.0), 5.0);
    assert_eq!(RotwkRules::claim_half_side(46.0, None, true, 10.0), 20.0);
}

#[test]
fn pathfind_diameter_overrides_the_claim_diameter() {
    assert_eq!(
        RotwkRules::claim_half_side(8.0, Some(40.0), false, 10.0),
        20.0
    );
    assert_eq!(RotwkRules::claim_half_side(8.0, None, false, 10.0), 5.0);
}

#[test]
fn an_archer_horde_does_not_close_on_a_target_inside_its_minimum_range() {
    use mint_sage_combat::{DamageNugget, Nugget, WeaponTemplate};
    let (mut combat, _bow, armour) = combat_data();
    let sword = retail_sword(&mut combat);
    let pierce = combat.damage_types.intern("PIERCE").unwrap();
    let bow = combat
        .weapons
        .insert(
            "MinRangeBow",
            WeaponTemplate {
                attack_range: 300.0,
                minimum_attack_range: 150.0,
                delay_between_shots: mint_sage_combat::FrameRange::fixed(
                    mint_sim::LogicFrames::from_frames(30),
                ),
                clip_size: 0,
                nuggets: vec![Nugget::Damage(DamageNugget {
                    damage: 1.0,
                    damage_type: pierce,
                    death_type: None,
                    sub_type: None,
                    radius: 0.0,
                    damage_scalar: 1.0,
                    delay: mint_sim::LogicFrames::ZERO,
                    arc: None,
                    arc_inverted: false,
                    flanking_bonus: 0.0,
                    requires_upgrade: false,
                    accepts_add: true,
                })],
                ..WeaponTemplate::default()
            },
        )
        .unwrap();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = block_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = |x: f32, weapon, owner| HordeSpawn {
        formation: 0,
        origin: Vec2::new(x, 300.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(10_000.0),
            weapon: Some(weapon),
            armour: Some(armour),
            ..Spawn::inert(
                Vec2::ZERO,
                Facing::EAST,
                Shape {
                    kind_of: KindOf::SELECTABLE,
                    radius: SOLDIER_RADIUS,
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
            vec![spawn(300.0, bow, PLAYER), spawn(420.0, sword, OTHER)],
        );
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    let start = centroid(&simulation, ids, &archers);
    let target_at = simulation.state().get(ids.body, target).unwrap().0.position;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    let towards = target_at.minus(start);
    let advanced = centroid(&simulation, ids, &archers)
        .minus(start)
        .dot(towards.scale(1.0 / towards.length()));
    assert!(
        advanced < 5.0,
        "the archers advanced {advanced:.1} into the melee"
    );
}

#[test]
fn a_ranged_horde_closes_along_the_straight_line_shortcut() {
    let (combat, bow, _armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let archer_formation = block_formation_with(None, vec![0, 1, 2, 3]);
    let archer_count = u32::try_from(archer_formation.len()).unwrap();
    let victim_formation = table_of(&[Vec2::ZERO]);
    let spawn = |formation: u16, x: f32, owner: PlayerId, weapon, count| HordeSpawn {
        formation,
        origin: Vec2::new(x, 300.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(100_000.0),
            weapon,
            armour: None,
            ..Spawn::inert(
                Vec2::ZERO,
                Facing::EAST,
                Shape {
                    kind_of: KindOf::SELECTABLE,
                    radius: SOLDIER_RADIUS,
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
            vec![archer_formation, victim_formation],
            vec![
                spawn(0, 300.0, PLAYER, Some(bow), archer_count),
                spawn(1, 530.0, OTHER, None, 1),
            ],
        )
        .with_horde_rangefinders(vec![Some(150.0), None]);
    let mut simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    let horde_entity = hordes[0];
    let start = simulation
        .state()
        .get(ids.body, horde_entity)
        .unwrap()
        .0
        .position;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let mut spot = None;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if let Some(path) = simulation.state().get(ids.path, horde_entity) {
            spot = Some(path.goal);
            break;
        }
    }
    let spot = spot.expect("the archers never planned a stand-off path");
    let distance = spot.minus(start).length();
    assert!(
        distance <= 45.5,
        "stand-off {distance:.2} is past the straight-line shortcut's 45-unit cap"
    );
    let steps = distance / 5.0;
    assert!(
        (steps - steps.round()).abs() < 0.05,
        "stand-off {distance:.2} is not a multiple of 5 (the straight-line shortcut's step)"
    );
}

fn archers_and_victims(victims: &[Vec2]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    archers_and_victims_with(victims, 1, false)
}

fn archers_and_victims_with(
    victims: &[Vec2],
    victim_size: u32,
    armed: bool,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let (combat, bow, _armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let archer_formation = block_formation_with(None, vec![0, 1, 2, 3]);
    let archer_count = u32::try_from(archer_formation.len()).unwrap();
    let victim_formation = table_of(
        &(0..victim_size)
            .map(|i| Vec2::new(0.0, 40.0 * i as f32))
            .collect::<Vec<_>>(),
    );
    let spawn = |formation: u16, origin: Vec2, owner: PlayerId, weapon, count| HordeSpawn {
        formation,
        origin,
        facing: Facing::EAST,
        member: Spawn {
            health: Some(100_000.0),
            weapon,
            armour: None,
            ..Spawn::inert(
                Vec2::ZERO,
                Facing::EAST,
                Shape {
                    kind_of: KindOf::SELECTABLE,
                    radius: SOLDIER_RADIUS,
                    height: 18.0,
                    geometry_is_small: false,
                },
                owner,
            )
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let mut spawns = vec![spawn(
        0,
        Vec2::new(200.0, 300.0),
        PLAYER,
        Some(bow),
        archer_count,
    )];
    spawns.extend(
        victims
            .iter()
            .map(|at| spawn(1, *at, OTHER, None, victim_size)),
    );
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(vec![archer_formation, victim_formation], spawns)
        .with_horde_rangefinders(vec![Some(BOW_RANGEFINDER), None])
        .with_horde_rangefinder_weapons(vec![armed.then_some(bow), None]);
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, hordes)
}

#[test]
fn a_ranged_horde_ordered_onto_another_victim_drops_the_old_route_at_once() {
    let (mut simulation, ids, hordes) =
        archers_and_victims(&[Vec2::new(510.0, 300.0), Vec2::new(200.0, 640.0)]);
    let members = |simulation: &Simulation<RotwkRules>, horde: Entity| {
        simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .clone()
    };
    let archers = members(&simulation, hordes[0]);
    let (first, second) = (
        members(&simulation, hordes[1])[0],
        members(&simulation, hordes[2])[0],
    );
    let attack = |simulation: &mut Simulation<RotwkRules>, target: Entity| {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: archers.clone(),
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target })
            .unwrap();
    };
    attack(&mut simulation, first);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let old = simulation.state().get(ids.path, hordes[0]).unwrap().goal;
    attack(&mut simulation, second);
    let heading = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.path, hordes[0])
            .map(|path| path.goal)
    };
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    let early = heading(&simulation);
    assert!(
        early.is_none_or(|goal| goal.y > old.y + 50.0),
        "three ticks after the new order the horde still heads for {early:?}, the old spot {old:?}"
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let later = heading(&simulation);
    assert!(
        later.is_some_and(|goal| goal.y > old.y + 50.0),
        "two seconds after the new order the horde heads for {later:?}"
    );
}

fn archers_against_a_walker(victim_x: f32, walk_to: f32) -> (f32, f32) {
    let (mut simulation, ids, hordes) = archers_and_victims(&[Vec2::new(victim_x, 300.0)]);
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    let position = |simulation: &Simulation<RotwkRules>, entity: Entity| {
        simulation.state().get(ids.body, entity).unwrap().0.position
    };
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![target],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: walk_to,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let start = position(&simulation, hordes[0]);
    let mut nearest = f32::MAX;
    for _ in 0..900 {
        simulation.tick().unwrap();
        let gap = position(&simulation, hordes[0])
            .minus(position(&simulation, target))
            .length();
        nearest = nearest.min(gap);
    }
    let travelled = position(&simulation, hordes[0]).minus(start).length();
    (nearest, travelled)
}

const BOW_RANGEFINDER: f32 = 150.0;

#[test]
fn a_ranged_horde_stops_where_its_weapon_first_reaches_a_target_walking_at_it() {
    let (nearest, _) = archers_against_a_walker(1000.0, 700.0);
    assert!(
        nearest >= BOW_RANGEFINDER,
        "the archers closed to {nearest:.1} of a victim their {BOW_RANGEFINDER} rangefinder reached from farther"
    );
}

#[test]
fn a_ranged_horde_replans_its_stand_off_spot_when_the_victim_walks_in() {
    let (_, still) = archers_against_a_walker(510.0, 510.0);
    let (_, walked_in) = archers_against_a_walker(510.0, 300.0);
    assert!(
        walked_in + 10.0 < still,
        "the archers walked {walked_in:.1} towards a victim walking at them, {still:.1} to one standing"
    );
}

fn archer_stand_off(gap: f32) -> (f32, f32) {
    let (probe, ids, hordes) = archers_and_victims(&[Vec2::new(900.0, 300.0)]);
    let footprint = probe
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .offsets()
        .fold(0.0f32, |farthest, offset| farthest.max(offset.length()))
        + SOLDIER_RADIUS;
    let victim_at = Vec2::new(200.0 + gap + footprint + SOLDIER_RADIUS, 300.0);
    let (mut simulation, ids, hordes) = archers_and_victims(&[victim_at]);
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers,
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let start = simulation
        .state()
        .get(ids.body, hordes[0])
        .unwrap()
        .0
        .position;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if let Some(path) = simulation.state().get(ids.path, hordes[0]) {
            let spot = path.goal;
            return (
                spot.minus(start).length(),
                victim_at.minus(spot).length() - footprint - SOLDIER_RADIUS,
            );
        }
    }
    panic!("the archers never planned a stand-off path");
}

#[test]
fn the_straight_line_spot_stands_a_little_inside_the_reach() {
    let (walked, _) = archer_stand_off(169.85);
    assert!(
        (walked - 25.0).abs() < 0.5,
        "the archers walked {walked:.2} to their spot, not the 25 of the step inside d^2 + 100 <= R^2"
    );
}

#[test]
fn the_cell_search_spot_is_judged_at_the_reach_itself() {
    let (walked, gap) = archer_stand_off(196.0);
    assert!(
        gap <= BOW_RANGEFINDER + 1e-3 && gap > BOW_RANGEFINDER - 12.0 && walked > 45.0,
        "the cell spot is {walked:.1} away and {gap:.2} from the victim"
    );
}

fn volley_log(gap: f32, victim_size: u32, ticks: usize) -> Vec<(bool, Vec<Option<Entity>>)> {
    let (probe, ids, hordes) = archers_and_victims(&[Vec2::new(900.0, 300.0)]);
    let footprint = probe
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .offsets()
        .fold(0.0f32, |farthest, offset| farthest.max(offset.length()))
        + SOLDIER_RADIUS;
    let victim_at = Vec2::new(
        200.0 + BOW_RANGEFINDER + gap + footprint + SOLDIER_RADIUS,
        300.0,
    );
    let (mut simulation, ids, hordes) = archers_and_victims_with(&[victim_at], victim_size, true);
    let archers = simulation
        .state()
        .get(ids.horde, hordes[0])
        .unwrap()
        .members
        .clone();
    let target = simulation
        .state()
        .get(ids.horde, hordes[1])
        .unwrap()
        .members[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: archers.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
    let last_shot = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.horde, hordes[0])
            .unwrap()
            .rangefinder
            .and_then(|state| state.last_shot())
    };
    let mut log = Vec::new();
    for _ in 0..ticks {
        let before = last_shot(&simulation);
        simulation.tick().unwrap();
        let held = archers
            .iter()
            .map(|member| {
                simulation
                    .state()
                    .get(ids.armed, *member)
                    .and_then(|armed| armed.target)
            })
            .collect();
        log.push((last_shot(&simulation) != before, held));
    }
    log
}

#[test]
fn a_ranged_hordes_members_take_victims_only_when_its_weapon_fires() {
    let log = volley_log(20.0, 1, 200);
    assert!(
        log.iter()
            .take(3)
            .all(|(_, held)| held.iter().all(Option::is_none)),
        "members held victims before the horde's weapon could fire"
    );
    let first = log
        .iter()
        .position(|(_, held)| held.iter().any(Option::is_some))
        .expect("no member ever took a victim");
    assert!(
        log[first].0,
        "the first victims were taken on tick {first}, which is not a tick the weapon fired"
    );
}

#[test]
fn a_member_keeps_its_victim_across_the_next_volleys() {
    let log = volley_log(-100.0, 2, 400);
    let volleys: Vec<usize> = log
        .iter()
        .enumerate()
        .filter_map(|(tick, (fired, _))| fired.then_some(tick))
        .collect();
    assert!(
        volleys.len() >= 4,
        "only {} volleys: {volleys:?}",
        volleys.len()
    );
    let first = &log[volleys[0]].1;
    assert!(
        first.iter().any(Option::is_some),
        "the first volley gave no victims"
    );
    for &tick in &volleys[1..4] {
        for (member, (before, after)) in first.iter().zip(&log[tick].1).enumerate() {
            assert!(
                before.is_none() || before == after,
                "member {member} was given another victim on the volley at tick {tick}"
            );
        }
    }
}

#[test]
fn a_group_move_replays_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, _, _) = column_of_hordes();
        (0..300)
            .map(|_| {
                simulation.tick().unwrap();
                simulation.state_hash()
            })
            .collect::<Vec<u64>>()
    };
    assert_eq!(run(), run());
}

fn hordes_at(origins: &[Vec2]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    hordes_at_with(origins, None)
}

fn hordes_at_with(
    origins: &[Vec2],
    count: Option<u32>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let (combat, _bow, armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = block_formation();
    let count = count.unwrap_or(u32::try_from(formation.len()).unwrap());
    let spawns = origins
        .iter()
        .map(|origin| HordeSpawn {
            formation: 0,
            origin: *origin,
            facing: Facing::EAST,
            member: Spawn {
                health: Some(200.0),
                armour: Some(armour),
                ..Spawn::inert(
                    Vec2::ZERO,
                    Facing::EAST,
                    Shape {
                        kind_of: KindOf::SELECTABLE,
                        radius: SOLDIER_RADIUS,
                        height: 18.0,
                        geometry_is_small: false,
                    },
                    PLAYER,
                )
            },
            count,
            vision: crate::shroud::HORDE_SHROUD_RANGE,
        })
        .collect();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_combat(combat)
        .with_hordes(vec![formation], spawns);
    let simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hordes = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    (simulation, ids, hordes)
}

fn order(simulation: &mut Simulation<RotwkRules>, ids: Ids, hordes: &[Entity], to: Vec2) {
    let units: Vec<Entity> = hordes
        .iter()
        .flat_map(|h| {
            simulation
                .state()
                .get(ids.horde, *h)
                .unwrap()
                .members
                .clone()
        })
        .collect();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: to.x,
                y: to.y,
                queue: false,
            },
        )
        .unwrap();
}

fn blocks(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    hordes: &[Entity],
) -> Vec<mint_movement::destination::Footprint> {
    let world = simulation.state();
    hordes
        .iter()
        .map(|h| {
            let body = world.get(ids.body, *h).unwrap().0;
            let block = crate::horde::horde_block(world, RotwkRules::horde_ids(ids), *h).unwrap();
            let forward = body.facing.to_vector();
            let left = Vec2::new(-forward.y, forward.x);
            let centre = body
                .position
                .plus(forward.scale(block.centre.x))
                .plus(left.scale(block.centre.y));
            mint_movement::destination::Footprint::new(
                centre,
                body.facing,
                block.half_along,
                block.half_across,
                block.member_radius,
            )
        })
        .collect()
}

fn squares_apart(
    a: &mint_movement::destination::Footprint,
    b: &mint_movement::destination::Footprint,
) -> bool {
    let apart = b.centre.minus(a.centre);
    apart.x.abs().max(apart.y.abs()) >= 90.0 - 2.0 * 5.0
}

#[test]
fn separate_orders_to_one_point_stand_beside_each_other_not_stacked() {
    let target = Vec2::new(500.0, 320.0);
    let (mut simulation, ids, hordes) = hordes_at(&[
        Vec2::new(150.0, 150.0),
        Vec2::new(150.0, 320.0),
        Vec2::new(150.0, 490.0),
    ]);
    for horde in &hordes {
        order(&mut simulation, ids, &[*horde], target);
        simulation.tick().unwrap();
        let goal = simulation.state().get(ids.path, *horde).map(|p| p.goal);
        println!("ordered horde {}: claimed {goal:?}", horde.index());
        for _ in 0..600 {
            simulation.tick().unwrap();
        }
    }
    let placed = blocks(&simulation, ids, &hordes);
    let report: Vec<String> = placed
        .iter()
        .map(|b| {
            format!(
                "({:.0}, {:.0}) facing ({:.2}, {:.2})",
                b.centre.x, b.centre.y, b.forward.x, b.forward.y
            )
        })
        .collect();
    for (i, a) in placed.iter().enumerate() {
        for (j, b) in placed.iter().enumerate().skip(i + 1) {
            println!("pair {i}-{j}: penetration {:.1}", a.penetration(b));
        }
    }
    println!("separate orders: block centres {report:?}");
    for (i, a) in placed.iter().enumerate() {
        for b in placed.iter().skip(i + 1) {
            assert!(
                a.penetration(b) < 2.0 * SOLDIER_RADIUS,
                "blocks stack: {report:?}"
            );
        }
    }
    for block in &placed {
        assert!(
            block.centre.minus(target).length() < 110.0,
            "a block wandered far from the point: {report:?}"
        );
    }
}

#[test]
fn a_smaller_horde_claims_a_smaller_block() {
    let block_of = |count: Option<u32>| {
        let (simulation, ids, hordes) = hordes_at_with(&[Vec2::new(300.0, 300.0)], count);
        crate::horde::horde_block(simulation.state(), RotwkRules::horde_ids(ids), hordes[0])
            .unwrap()
    };
    let full = block_of(None);
    let ten = block_of(Some(10));
    println!("block full {full:?}, ten members {ten:?}");
    assert!((full.half_along - 28.0).abs() < 2.0, "{full:?}");
    assert!((full.half_across - 48.0).abs() < 2.0, "{full:?}");
    assert!(
        ten.half_along * ten.half_across < full.half_along * full.half_across - 200.0,
        "ten members claim less ground than fifteen: {ten:?}"
    );
}

#[test]
fn hordes_ordered_onto_one_point_stand_closer_than_a_whole_square_apart() {
    let (mut simulation, ids, hordes) =
        hordes_at(&[Vec2::new(150.0, 300.0), Vec2::new(150.0, 500.0)]);
    order(&mut simulation, ids, &[hordes[0]], Vec2::new(600.0, 350.0));
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    order(&mut simulation, ids, &[hordes[1]], Vec2::new(600.0, 350.0));
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let placed = blocks(&simulation, ids, &hordes);
    let distance = placed[0].centre.minus(placed[1].centre).length();
    let depth = placed[0].penetration(&placed[1]);
    println!(
        "group move to one point: centres ({:.0}, {:.0}) facing {:?}, ({:.0}, {:.0}) facing {:?}, \
         distance {distance:.1}, penetration {depth:.1}",
        placed[0].centre.x,
        placed[0].centre.y,
        placed[0].forward,
        placed[1].centre.x,
        placed[1].centre.y,
        placed[1].forward,
    );
    assert!(distance < 85.0, "stood a whole square apart: {distance:.1}");
    assert!(depth < 2.0 * SOLDIER_RADIUS, "hordes merge: {depth:.1}");
}

#[test]
fn separate_orders_replay_to_the_same_hash_sequence() {
    let run = || {
        let (mut simulation, ids, hordes) =
            hordes_at(&[Vec2::new(150.0, 200.0), Vec2::new(150.0, 400.0)]);
        let mut hashes = Vec::new();
        for horde in &hordes {
            order(&mut simulation, ids, &[*horde], Vec2::new(450.0, 300.0));
            for _ in 0..150 {
                simulation.tick().unwrap();
                hashes.push(simulation.state_hash());
            }
        }
        hashes
    };
    assert_eq!(run(), run());
}
