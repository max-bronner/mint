#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use mint_sage_autoresolve::tables::UnitType;
use mint_sage_livingworld::{Army as StratArmy, RegionId, SpecSide, UnitId};

use super::*;

fn gated(name: &str) -> Vec<Gated> {
    vec![Gated {
        required: Vec::new(),
        excluded: Vec::new(),
        name: name.to_owned(),
    }]
}

fn template(t: &mut Tables, name: &str, weapon: (&str, f32), hp: f32) {
    let mut w = Weapon {
        miss_percent: 0,
        ..Weapon::default()
    };
    w.damage[UnitType::Soldier.index()] = weapon.1;
    t.weapons.insert(weapon.0.to_owned(), w);
    t.bodies.insert(
        format!("{name}_body"),
        Body {
            hitpoints: BTreeMap::from([(1, hp)]),
            ..Body::default()
        },
    );
    t.templates.insert(
        name.to_owned(),
        Template {
            unit_type: UnitType::Soldier,
            weapons: gated(weapon.0),
            armors: Vec::new(),
            body: Some(format!("{name}_body")),
            leadership: None,
            chain: None,
            trainable: true,
            horde: false,
            structure: false,
            hero: false,
            initial_payload: Vec::new(),
            levels: Vec::new(),
        },
    );
}

fn resolver() -> InstallResolver {
    let mut tables = Tables::default();
    template(&mut tables, "elf", ("elf_weapon", 1.0), 100.0);
    template(&mut tables, "orc", ("orc_weapon", 900.0), 1000.0);
    InstallResolver::with(tables, vec!["elf".to_owned(), "orc".to_owned()])
}

fn army(id: u32, player: u8, unit: u16, member: u32, count: u32) -> StratArmy {
    StratArmy {
        id: ArmyId(id),
        player: PlayerId(player),
        hero: None,
        roster: vec![StratMember {
            id: member,
            unit: UnitId(unit),
            level: 1,
            count,
            experience: 0,
            battles: 0,
            kills: 0,
            kills_battle: 0,
            upgraded_at: None,
        }],
        region: RegionId(0),
        prev_region: None,
        turns_here: 0,
        order: None,
        destroy_queued: false,
    }
}

fn spec() -> BattleSpec {
    BattleSpec {
        battle: 5,
        region: RegionId(0),
        owner: Some(PlayerId(0)),
        sides: vec![
            SpecSide {
                players: vec![PlayerId(1)],
                armies: vec![army(11, 1, 1, 101, 2)],
            },
            SpecSide {
                players: vec![PlayerId(0)],
                armies: vec![army(10, 0, 0, 100, 2)],
            },
        ],
        map: None,
        region_name: String::new(),
        neighbours: Vec::new(),
        participants: Vec::new(),
        seed: 0,
        weapon_scale: Vec::new(),
        round_cap: None,
        revival_multipliers: (1.0, 1.0),
    }
}

#[test]
fn the_owner_defends_whatever_the_order_of_the_sides_and_the_winner_names_the_spec_side() {
    let mut resolver = resolver();
    let result = resolver.resolve(&spec(), &mut Pcg32::new(1));
    assert_eq!(result.winner, Some(0), "the orcs, spec side 0, win");
    let last = resolver.last.as_ref().unwrap();
    assert_eq!(last.sides, [vec![PlayerId(0)], vec![PlayerId(1)]]);
    assert_eq!(last.outcome.winner, 1);
}

fn stack_against_a_guard() -> (InstallResolver, BattleSpec) {
    let mut tables = Tables::default();
    template(&mut tables, "guard", ("guard_weapon", 600.0), 500.0);
    template(&mut tables, "raider", ("raider_weapon", 10.0), 100.0);
    tables.schedules[1] = Schedule {
        rounds: BTreeMap::from([(1, 0), (2, 15)]),
        each_remaining: 1,
    };
    let resolver = InstallResolver::with(tables, vec!["guard".to_owned(), "raider".to_owned()]);
    let led = |id: u32, members: u32| {
        let one = army(id, 1, 1, 0, 1);
        StratArmy {
            hero: Some(UnitId(1)),
            roster: (0..members)
                .map(|i| StratMember {
                    id: id * 100 + i,
                    ..one.roster[0].clone()
                })
                .collect(),
            ..one
        }
    };
    let spec = BattleSpec {
        sides: vec![
            SpecSide {
                players: vec![PlayerId(1)],
                armies: vec![led(11, 1), led(12, 10)],
            },
            SpecSide {
                players: vec![PlayerId(0)],
                armies: vec![army(10, 0, 0, 100, 1)],
            },
        ],
        ..spec()
    };
    (resolver, spec)
}

#[test]
fn the_ai_predicts_with_the_resolver_so_a_stack_whose_first_army_falls_alone_is_seen_to_lose() {
    let (mut resolver, spec) = stack_against_a_guard();
    let predicted = resolver
        .forecast()
        .predict(&spec, 0, None, &mut Pcg32::new(3));
    assert_eq!(
        predicted,
        Prediction {
            winner: Some(1),
            rounds: 1
        },
        "the guard, spec side 1, wins before the ten raiders arrive"
    );
    let result = resolver.resolve(&spec, &mut Pcg32::new(3));
    assert_eq!(result.winner, predicted.winner);
    assert_eq!(
        resolver.last.as_ref().map(|b| b.outcome.rounds),
        Some(predicted.rounds)
    );
}

#[test]
fn survivors_come_back_as_their_armys_new_roster_with_their_member_ids() {
    let mut resolver = resolver();
    let result = resolver.resolve(&spec(), &mut Pcg32::new(1));
    let orcs = result
        .rosters
        .iter()
        .find(|(army, _)| *army == ArmyId(11))
        .map(|(_, roster)| roster)
        .unwrap();
    assert_eq!(orcs.iter().map(|m| m.id).collect::<Vec<_>>(), vec![101]);
    assert_eq!(orcs[0].unit, UnitId(1));
    let elves = result
        .rosters
        .iter()
        .find(|(army, _)| *army == ArmyId(10))
        .map(|(_, roster)| roster)
        .unwrap();
    assert!(elves.is_empty(), "the defeated army keeps no one");
}

#[test]
fn the_last_round_of_the_log_names_the_units_the_survivors_number() {
    let mut resolver = resolver();
    resolver.resolve(&spec(), &mut Pcg32::new(1));
    let last = resolver.last.as_ref().unwrap();
    let log = last.log().last().unwrap();
    assert_eq!(log.units, last.survivors());
    assert!(!last.log().is_empty());
}

#[test]
fn each_participant_is_tracked_and_the_per_participant_hit_points_add_up_to_the_sides() {
    let mut resolver = resolver();
    resolver.resolve(&spec(), &mut Pcg32::new(1));
    let last = resolver.last.as_ref().unwrap();
    let outcome = &last.outcome;
    assert_eq!(outcome.fields.len(), last.order.len());
    for round in 0..=outcome.log.len() {
        let mut by_side = [0.0f32; 2];
        for (participant, track) in outcome.fields.iter().enumerate() {
            by_side[last.side_of(participant)] += track[round].hit_points;
        }
        let whole = if round == 0 {
            outcome.opening
        } else {
            outcome.log[round - 1]
        };
        for side in 0..2 {
            assert!(
                (by_side[side] - whole.hit_points[side]).abs() < 0.5,
                "round {round} side {side}: {by_side:?} against {:?}",
                whole.hit_points
            );
        }
    }
}

#[test]
fn a_zero_weapon_scale_and_a_round_cap_make_a_one_sided_capped_fight() {
    let mut resolver = resolver();
    let mut pursuit = spec();
    pursuit.weapon_scale.push((PlayerId(1), 0.0));
    pursuit.round_cap = Some(3);
    resolver.resolve(&pursuit, &mut Pcg32::new(1));
    let last = resolver.last.as_ref().unwrap();
    assert_eq!(last.outcome.rounds, 3);
    let defender = last
        .outcome
        .participants
        .first()
        .and_then(|p| p.armies.first())
        .unwrap();
    assert_eq!(
        defender.survivors.len(),
        1,
        "unhurt: the zeroed side deals nothing"
    );
}
