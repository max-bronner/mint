#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::arithmetic_side_effects
)]

use std::collections::BTreeMap;

use mint_math::rng::Pcg32;

use super::*;
use crate::modifiers::{multipliers, territory_share, BonusInputs, Percents};
use crate::roster::{Army, RankStep};
use crate::tables::UnitType::*;
use crate::tables::{ExperienceLevel, Gated, Schedule};

fn gated(name: &str) -> Vec<Gated> {
    vec![Gated {
        required: Vec::new(),
        excluded: Vec::new(),
        name: name.to_owned(),
    }]
}

fn weapon(t: &mut Tables, name: &str, miss: i32, hurt: bool, damage: &[(UnitType, f32)]) {
    let mut w = Weapon {
        miss_percent: miss,
        reduce_attack_when_hurt: hurt,
        ..Weapon::default()
    };
    for (ty, d) in damage {
        w.damage[ty.index()] = *d;
    }
    t.weapons.insert(name.to_owned(), w);
}

fn armor(t: &mut Tables, name: &str, taken: &[(UnitType, f32)]) {
    let mut a = Armor::default();
    for (ty, f) in taken {
        a.taken[ty.index()] = *f;
    }
    t.armors.insert(name.to_owned(), a);
}

fn body(t: &mut Tables, name: &str, hp: f32, leave: bool) {
    t.bodies.insert(
        name.to_owned(),
        Body {
            hitpoints: BTreeMap::from([(1, hp)]),
            leave_in_army_summary: leave,
            ..Body::default()
        },
    );
}

fn template(t: &mut Tables, name: &str, ty: UnitType, w: &str, a: &str, b: &str) {
    t.templates.insert(
        name.to_owned(),
        Template {
            unit_type: ty,
            weapons: gated(w),
            armors: gated(a),
            body: Some(b.to_owned()),
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

fn tpl<'t>(t: &'t mut Tables, name: &str) -> &'t mut Template {
    t.templates.get_mut(name).unwrap()
}

fn side(side: usize, units: &[&str]) -> Participant {
    Participant {
        side,
        weapon_multiplier: 1.0,
        armor_multiplier: 1.0,
        experience_multiplier: 1.0,
        skill: None,
        legendary_bonus: 0,
        science_bonus: Vec::new(),
        initial_units: units.iter().map(|u| Member::new(u, 1)).collect(),
        armies: Vec::new(),
    }
}

const TUTORIAL: Options = Options {
    tutorial: true,
    round_cap: None,
};

fn soldier_vs_archer() -> Tables {
    let mut t = Tables::default();
    weapon(&mut t, "sw", 0, true, &[(Archer, 75.0)]);
    weapon(&mut t, "aw", 0, true, &[(Soldier, 70.0)]);
    armor(&mut t, "sa", &[(Archer, 2.0)]);
    armor(&mut t, "aa", &[(Soldier, 0.5)]);
    body(&mut t, "sb", 1650.0, false);
    body(&mut t, "ab", 1500.0, false);
    template(&mut t, "soldier", Soldier, "sw", "sa", "sb");
    template(&mut t, "archer", Archer, "aw", "aa", "ab");
    t
}

fn hp(battle: &Battle<'_>, template: &str) -> f32 {
    battle
        .units
        .iter()
        .find(|u| u.member.template == template)
        .unwrap()
        .hp
}

#[test]
fn damage_is_computed_in_f32_from_round_start_hit_points() {
    let t = soldier_vs_archer();
    let ps = [side(0, &["soldier"]), side(1, &["archer"])];
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.step_round();
    assert_eq!((hp(&b, "soldier"), hp(&b, "archer")), (1510.0, 1462.5));
    b.step_round();
    assert_eq!(
        (f64::from(hp(&b, "soldier")), f64::from(hp(&b, "archer"))),
        (1_373.5, 1_428.181_762_695_312_5)
    );
    b.step_round();
    assert_eq!(
        (f64::from(hp(&b, "soldier")), f64::from(hp(&b, "archer"))),
        (1_240.203_002_929_687_5, 1_396.965_820_312_5)
    );
}

#[test]
fn the_hit_point_ratio_is_rounded_to_f32_before_it_scales_the_damage() {
    let mut w = Weapon::default();
    w.damage[Archer.index()] = 75.0;
    let d = w.damage_against(1, 1510.0, 1650.0, Archer);
    assert_eq!(f64::from(d), 68.636_367_797_851_56);
}

#[test]
fn level_bonus_applies_only_from_its_level() {
    let mut w = Weapon::default();
    w.damage[Soldier.index()] = 70.0;
    w.level_bonus.insert(2, 0.10);
    assert_eq!(w.damage_against(2, 100.0, 100.0, Soldier), 77.0);
    assert_eq!(w.damage_against(1, 100.0, 100.0, Soldier), 70.0);
}

#[test]
fn defense_and_castle_rows_multiply_in_retail_order() {
    let m = multipliers(&BonusInputs {
        player: Percents {
            defense: 5,
            ..Percents::default()
        },
        strengthen: vec![crate::modifiers::Strengthen {
            count: 2,
            rows: vec![crate::modifiers::StrengthenRow {
                count: 2,
                weapon: 1.0,
                armor: 0.81,
                experience: 1.0,
            }],
        }],
        ..BonusInputs::default()
    });
    assert_eq!(f64::from(m.armor), 0.771_428_644_657_135);
    let plain = multipliers(&BonusInputs {
        player: Percents {
            defense: 5,
            ..Percents::default()
        },
        ..BonusInputs::default()
    });
    assert_eq!(f64::from(plain.armor), 0.952_381_014_823_913_6);
}

#[test]
fn territory_share_truncates_the_scaled_bonus() {
    assert_eq!(territory_share(15, 2, 3), 10);
}

#[test]
fn the_dead_territory_factors_change_nothing() {
    let m = multipliers(&BonusInputs {
        extra_start_resources: 1000,
        free_builder: 1,
        ..BonusInputs::default()
    });
    assert_eq!((m.weapon, m.armor, m.experience), (1.0, 1.0, 1.0));
}

#[test]
fn units_that_kill_each_other_both_fire_and_the_defender_wins() {
    let mut t = Tables::default();
    weapon(&mut t, "w", 0, false, &[(Soldier, 100.0)]);
    body(&mut t, "b", 10.0, false);
    template(&mut t, "u", Soldier, "w", "none", "b");
    let ps = [side(0, &["u"]), side(1, &["u"])];
    let mut rng = Pcg32::new(1);
    let out = resolve(&ps, &t, TUTORIAL, &mut rng);
    assert_eq!((out.winner, out.rounds, out.kills.len()), (0, 1, 2));
}

fn chain_fixture() -> Tables {
    let mut t = Tables::default();
    weapon(&mut t, "hit", 0, false, &[(Hero, 1.0), (Soldier, 1.0)]);
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "b", 100.0, false);
    template(&mut t, "a", Soldier, "hit", "x", "b");
    template(&mut t, "b", Soldier, "hit", "x", "b");
    template(&mut t, "hero", Hero, "none", "x", "b");
    template(&mut t, "soldier", Soldier, "none", "x", "b");
    for (name, pri) in [("a", 50), ("b", 25)] {
        let mut c = Chain::default();
        c.name(Hero, pri).unwrap();
        t.chains.insert(name.to_owned(), c);
        tpl(&mut t, name).chain = Some(name.to_owned());
    }
    t
}

fn target_of(b: &Battle<'_>, template: &str) -> String {
    let u = b
        .units
        .iter()
        .find(|u| u.member.template == template)
        .unwrap();
    b.units[u.target.unwrap()].member.template.clone()
}

#[test]
fn the_attacker_with_the_lowest_priority_picks_first() {
    let t = chain_fixture();
    let ps = [side(0, &["a", "b"]), side(1, &["hero", "soldier"])];
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.assign_targets();
    assert_eq!(target_of(&b, "b"), "hero");
    assert_eq!(target_of(&b, "a"), "soldier");
}

#[test]
fn attackers_are_shared_out_by_quota() {
    let mut t = Tables::default();
    weapon(&mut t, "hit", 0, false, &[(Soldier, 1.0)]);
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "b", 100.0, false);
    template(&mut t, "atk", Soldier, "hit", "x", "b");
    template(&mut t, "def", Soldier, "none", "x", "b");
    let ps = [side(0, &["atk"; 5]), side(1, &["def"; 2])];
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.assign_targets();
    let mut counts = BTreeMap::new();
    for u in b.units.iter().filter_map(|u| u.target) {
        *counts.entry(u).or_insert(0) += 1;
    }
    let mut counts: Vec<i32> = counts.into_values().collect();
    counts.sort_unstable();
    assert_eq!(counts, [2, 3]);
}

#[test]
fn a_shot_kept_after_its_target_died_goes_to_a_new_target_the_same_round() {
    let mut t = Tables::default();
    for (name, d) in [("big", 1000.0), ("mid", 5.0), ("small", 3.0)] {
        weapon(&mut t, name, 0, false, &[(Soldier, d)]);
        template(&mut t, name, Soldier, name, "x", "tough");
    }
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "tough", 1000.0, false);
    body(&mut t, "weak", 10.0, false);
    body(&mut t, "mid_hp", 100.0, false);
    template(&mut t, "t1", Soldier, "none", "x", "weak");
    template(&mut t, "t2", Soldier, "none", "x", "mid_hp");
    let ps = [side(0, &["big", "mid", "small"]), side(1, &["t1", "t2"])];
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.step_round();
    assert_eq!(hp(&b, "t2"), 92.0);
}

fn leadership_fixture(leader_hp: f32) -> (Tables, Vec<Participant>) {
    let mut t = Tables::default();
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "b", leader_hp.max(1.0), true);
    template(&mut t, "leader", Soldier, "none", "x", "b");
    template(&mut t, "sub", Soldier, "none", "x", "b");
    tpl(&mut t, "leader").leadership = Some("l".to_owned());
    t.leaderships.insert(
        "l".to_owned(),
        Leadership {
            affects: vec![Soldier],
            affects_higher_level_first: true,
            bonuses: BTreeMap::from([(
                1,
                LeadershipBonus {
                    maximum_units_affected: 2,
                    priority: 1,
                    weapon: 1.5,
                    armor: 1.0,
                    experience: 1.0,
                },
            )]),
        },
    );
    let mut p = side(0, &["leader", "sub", "sub", "sub", "sub"]);
    for (m, level) in p.initial_units.iter_mut().zip([5, 5, 3, 3, 1]) {
        m.level = level;
    }
    (t, vec![p, side(1, &["sub"])])
}

fn boosted(b: &Battle<'_>) -> Vec<bool> {
    b.sides[0]
        .iter()
        .map(|&u| b.units[u].bonus.is_some())
        .collect()
}

#[test]
fn a_leader_boosts_the_first_eligible_units_but_never_itself() {
    let (t, ps) = leadership_fixture(100.0);
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.assign_leadership(0);
    assert_eq!(boosted(&b), [false, true, true, false, false]);
}

#[test]
fn a_leader_kept_at_zero_hit_points_still_leads() {
    let (t, ps) = leadership_fixture(100.0);
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[0].hp = 0.0;
    b.assign_leadership(0);
    assert_eq!(boosted(&b), [false, true, true, false, false]);
}

#[test]
fn reinforcement_rounds_follow_the_army_index() {
    let s = Schedule {
        rounds: BTreeMap::from([(1, 0), (2, 15), (3, 30), (4, 45)]),
        each_remaining: 1,
    };
    let rounds: Vec<i32> = (0..6).map(|k| s.round_of(k)).collect();
    assert_eq!(rounds, [0, 15, 30, 45, 46, 47]);
}

fn write_back_fixture() -> (Tables, Vec<Participant>) {
    let mut t = Tables::default();
    weapon(&mut t, "grunt", 0, false, &[(Soldier, 60.0)]);
    weapon(&mut t, "foe", 0, false, &[(Soldier, 100.0)]);
    body(&mut t, "gb", 100.0, false);
    body(&mut t, "fb", 150.0, false);
    template(&mut t, "grunt", Soldier, "grunt", "x", "gb");
    template(&mut t, "foe", Soldier, "foe", "x", "fb");
    for name in ["grunt", "foe"] {
        let tp = tpl(&mut t, name);
        tp.levels = vec![ExperienceLevel {
            required: 0.0,
            award: 10,
            rank: 1,
        }];
    }
    t.schedules[0] = Schedule {
        rounds: BTreeMap::from([(1, 0), (2, 15)]),
        each_remaining: 0,
    };
    let army = |id, named| Army {
        id,
        named,
        members: vec![Member::new("grunt", 1); 3],
    };
    let mut owner = side(0, &[]);
    owner.armies = vec![army(7, true), army(8, true)];
    let mut enemy = side(1, &["foe"]);
    enemy.skill = Some(SkillTracker {
        points: 5.0,
        purchase_points: 0,
        next_rank: 0,
        ranks: vec![
            RankStep {
                threshold: 6.0,
                purchase_points_granted: 2,
            },
            RankStep {
                threshold: 100.0,
                purchase_points_granted: 0,
            },
        ],
        gain_scale: 1.0,
        base_rank: 0,
        scaling: Vec::new(),
    });
    (t, vec![owner, enemy])
}

#[test]
fn survivors_replace_the_army_that_fought_and_later_armies_are_untouched() {
    let (t, ps) = write_back_fixture();
    let mut rng = Pcg32::new(1);
    let out = resolve(&ps, &t, TUTORIAL, &mut rng);
    let armies = &out.participants[0].armies;
    assert_eq!((out.winner, out.rounds), (0, 1));
    assert_eq!(
        (
            armies[0].touched,
            armies[0].survivors.len(),
            armies[1].touched
        ),
        (true, 2, false)
    );
}

#[test]
fn each_participant_is_tracked_per_round_and_its_later_batches_are_listed_even_if_never_fought() {
    let (t, ps) = write_back_fixture();
    let mut rng = Pcg32::new(1);
    let out = resolve(&ps, &t, TUTORIAL, &mut rng);
    assert_eq!(out.rounds, 1);
    assert_eq!(out.fields.len(), 2);
    assert!(out.fields.iter().all(|f| f.len() == 2));
    assert_eq!(out.fields[0][0].hit_points, 300.0);
    assert_eq!(out.fields[1][0].hit_points, 150.0);
    for (side, track) in out.fields.iter().enumerate() {
        assert_eq!(track[0].hit_points, out.opening.hit_points[side]);
        assert_eq!(track[1].hit_points, out.log[0].hit_points[side]);
        assert_eq!(track[1].units, out.log[0].units[side]);
    }
    let rounds = |p: usize| -> Vec<(u32, u32, f32)> {
        out.arrivals[p]
            .iter()
            .map(|a| (a.army, a.round, a.hit_points))
            .collect()
    };
    assert_eq!(rounds(0), vec![(7, 0, 300.0), (8, 15, 300.0)]);
    assert!(rounds(1).is_empty());
}

#[test]
fn skill_points_won_in_the_battle_are_not_written_back() {
    let (t, ps) = write_back_fixture();
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    while b.winner.is_none() {
        b.step_round();
    }
    let earned = b.skills[1].as_ref().unwrap().clone();
    let out = b.finish();
    assert_eq!((earned.points, earned.purchase_points), (15.0, 2));
    assert_eq!(
        (
            out.participants[1].skill_points,
            out.participants[1].purchase_points_delta
        ),
        (Some(5.0), Some(0))
    );
    assert!(out.participants[1].lost);
}

fn cap_run(defender: usize, attacker: usize) -> Outcome {
    let mut t = Tables::default();
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "b", 10.0, false);
    template(&mut t, "u", Soldier, "none", "x", "b");
    let ps = [side(0, &vec!["u"; defender]), side(1, &vec!["u"; attacker])];
    let mut rng = Pcg32::new(1);
    resolve(&ps, &t, TUTORIAL, &mut rng)
}

#[test]
fn at_the_round_cap_more_units_win_and_a_tie_goes_to_side_zero() {
    let winners = [cap_run(1, 2), cap_run(2, 2), cap_run(2, 1)].map(|o| (o.winner, o.rounds));
    assert_eq!(winners, [(1, 1000), (0, 1000), (0, 1000)]);
}

fn regen_battle<'a>(ps: &'a [Participant], t: &'a Tables, rng: &'a mut Pcg32) -> Battle<'a> {
    Battle::new(ps, t, TUTORIAL, rng)
}

fn regen_tables() -> Tables {
    let mut t = Tables::default();
    weapon(&mut t, "none", 0, false, &[]);
    t.bodies.insert(
        "b".to_owned(),
        Body {
            hitpoints: BTreeMap::from([(1, 100.0), (2, 200.0)]),
            regenerate: BTreeMap::from([(1, 5.0), (2, 5.0)]),
            ..Body::default()
        },
    );
    template(&mut t, "u", Soldier, "none", "x", "b");
    t
}

#[test]
fn regeneration_is_clamped_stops_at_round_500_and_follows_a_level_up() {
    let t = regen_tables();
    let ps = [side(0, &["u"]), side(1, &["u"])];
    let mut rng = Pcg32::new(1);
    let mut b = regen_battle(&ps, &t, &mut rng);
    b.units[0].hp = 50.0;
    b.step_round();
    assert_eq!(b.units[0].hp, 55.0);
    b.units[0].hp = 98.0;
    b.step_round();
    assert_eq!(b.units[0].hp, 100.0);
    b.round = REGENERATION_ROUNDS;
    b.units[0].hp = 50.0;
    b.step_round();
    assert_eq!(b.units[0].hp, 50.0);
    b.round = 1;
    b.units[0].max_hp = 100.0;
    b.units[0].member.level = 2;
    b.units[0].hp = 50.0;
    b.end_of_round();
    assert_eq!(b.units[0].hp, 105.0);
}

#[test]
fn one_draw_is_taken_per_shot_and_none_in_a_tutorial() {
    let t = soldier_vs_archer();
    let ps = [side(0, &["soldier"]), side(1, &["archer"])];
    let live = resolve(&ps, &t, Options::default(), &mut Pcg32::new(3));
    let tutorial = resolve(&ps, &t, TUTORIAL, &mut Pcg32::new(3));
    let shots = live.rounds * 2;
    assert_eq!((live.draws, tutorial.draws), (shots, 0));
}

#[test]
fn a_fixed_seed_gives_identical_results_twice() {
    let (mut t, ps) = write_back_fixture();
    for w in t.weapons.values_mut() {
        w.miss_percent = 50;
    }
    let run = || resolve(&ps, &t, Options::default(), &mut Pcg32::new(12345));
    let (a, b) = (run(), run());
    assert!(a.draws > 0);
    assert_eq!(a, b);
}

fn kill_fixture() -> (Tables, Vec<Participant>) {
    let mut t = Tables::default();
    weapon(&mut t, "none", 0, false, &[]);
    body(&mut t, "b", 10.0, false);
    template(&mut t, "att", Soldier, "none", "x", "b");
    template(&mut t, "vic", Soldier, "none", "x", "b");
    tpl(&mut t, "att").levels = vec![
        ExperienceLevel {
            required: 0.0,
            award: 0,
            rank: 1,
        },
        ExperienceLevel {
            required: 100.0,
            award: 0,
            rank: 2,
        },
        ExperienceLevel {
            required: 300.0,
            award: 0,
            rank: 3,
        },
    ];
    tpl(&mut t, "vic").levels = vec![
        ExperienceLevel {
            required: 0.0,
            award: 20,
            rank: 1,
        },
        ExperienceLevel {
            required: 100.0,
            award: 40,
            rank: 2,
        },
    ];
    let mut owner = side(0, &["att"]);
    owner.skill = Some(SkillTracker {
        points: 0.0,
        purchase_points: 0,
        next_rank: 0,
        ranks: Vec::new(),
        gain_scale: 1.0,
        base_rank: 0,
        scaling: Vec::new(),
    });
    (t, vec![owner, side(1, &["vic"])])
}

fn award_with(experience: f32, quantity: i32) -> i32 {
    let (t, ps) = kill_fixture();
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[1].member.experience = experience;
    b.units[1].member.quantity = quantity;
    b.award(1)
}

#[test]
fn the_award_truncates_experience_and_scales_with_quantity() {
    assert_eq!(award_with(150.0, 1), 40);
    assert_eq!(award_with(99.9, 1), 20);
    assert_eq!(award_with(150.0, 3), 120);
    assert_eq!(award_with(150.0, 0), 0);
}

#[test]
fn a_horde_awards_by_its_payload_and_ignores_its_own_levels() {
    let (mut t, ps) = kill_fixture();
    template(&mut t, "fighter", Soldier, "none", "x", "b");
    tpl(&mut t, "fighter").levels = vec![ExperienceLevel {
        required: 0.0,
        award: 10,
        rank: 1,
    }];
    let h = tpl(&mut t, "vic");
    h.horde = true;
    h.initial_payload = vec![("fighter".to_owned(), 5)];
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[1].member.quantity = 2;
    assert_eq!(b.award(1), 100);
}

#[test]
fn kill_experience_applies_the_participant_factor_then_leadership_after_the_skill_gain() {
    let (t, mut ps) = kill_fixture();
    ps[0].experience_multiplier = multipliers(&BonusInputs {
        player: Percents {
            experience: 5,
            ..Percents::default()
        },
        ..BonusInputs::default()
    })
    .experience;
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[1].member.experience = 0.0;
    b.units[1].member.level = 1;
    b.units[1].member.quantity = 1;
    tpl_award(&mut b, 25);
    b.credit_kill(0, 1);
    assert_eq!(b.units[0].member.experience, 26.0);

    let (t, mut ps) = kill_fixture();
    ps[0].experience_multiplier = 1.0;
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[0].bonus = Some(LeadershipBonus {
        maximum_units_affected: 0,
        priority: 0,
        weapon: 1.0,
        armor: 1.0,
        experience: 1.5,
    });
    tpl_award(&mut b, 40);
    b.credit_kill(0, 1);
    assert_eq!(b.units[0].member.experience, 60.0);
    assert_eq!(b.skills[0].as_ref().unwrap().points, 40.0);
}

fn tpl_award(b: &mut Battle<'_>, award: i32) {
    let mut template = b.units[1].template.clone();
    template.levels = vec![ExperienceLevel {
        required: 0.0,
        award,
        rank: 1,
    }];
    let leaked: &'static Template = Box::leak(Box::new(template));
    b.units[1].template = leaked;
}

#[test]
fn a_kill_can_raise_a_unit_several_levels_at_once() {
    let (t, ps) = kill_fixture();
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    b.units[0].member.experience = 90.0;
    add_experience(&mut b.units[0], 220.0);
    assert_eq!(
        (b.units[0].member.experience, b.units[0].member.level),
        (310.0, 3)
    );
}

#[test]
fn an_untrainable_unit_gains_nothing() {
    let (mut t, ps) = kill_fixture();
    tpl(&mut t, "att").trainable = false;
    let mut rng = Pcg32::new(1);
    let mut b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    add_experience(&mut b.units[0], 50.0);
    assert_eq!(b.units[0].member.experience, 0.0);
}

fn walk(chain: &Chain) -> Vec<(UnitType, i32)> {
    let mut out = Vec::new();
    let mut at = chain.first();
    while let Some((ty, pri)) = at {
        out.push((ty, pri));
        at = chain.next(ty, pri);
    }
    out
}

#[test]
fn equal_priorities_take_the_higher_type_first_then_the_unnamed_band() {
    let mut c = Chain::default();
    c.name(Fortress, 50).unwrap();
    for ty in [Soldier, Hero, Cavalry] {
        c.name(ty, 25).unwrap();
    }
    let order: Vec<UnitType> = walk(&c).into_iter().map(|s| s.0).collect();
    assert_eq!(
        order,
        [Fortress, Hero, Cavalry, Soldier, Archer, Pikemen, Monster, Siege, Support, Invalid]
    );
}

#[test]
fn negative_priorities_come_after_the_unnamed_band() {
    let mut c = Chain::default();
    c.name(Siege, -5).unwrap();
    c.name(Archer, -9).unwrap();
    let order: Vec<UnitType> = walk(&c).into_iter().map(|s| s.0).collect();
    assert_eq!(
        order,
        [Soldier, Pikemen, Cavalry, Monster, Hero, Fortress, Support, Invalid, Siege, Archer]
    );
}

#[test]
fn priority_zero_leaves_the_type_unnamed_and_a_repeat_is_an_error() {
    let mut c = Chain::default();
    c.name(Soldier, 0).unwrap();
    c.name(Soldier, 0).unwrap();
    assert_eq!(walk(&c).first(), Some(&(Soldier, 0)));
    c.name(Hero, 5).unwrap();
    assert_eq!(c.name(Hero, 7), Err(crate::chain::Redefined(Hero)));
    assert_eq!(c.name(Hero, 0), Err(crate::chain::Redefined(Hero)));
}

#[test]
fn a_chain_with_every_type_named_below_zero_offers_nothing() {
    let mut c = Chain::default();
    for ty in UnitType::ALL {
        c.name(ty, -1).unwrap();
    }
    assert_eq!(c.first(), None);
}

#[test]
fn the_invalid_type_is_reachable_after_the_last_named_type() {
    let mut c = Chain::default();
    for ty in &UnitType::ALL[..9] {
        c.name(*ty, 3).unwrap();
    }
    assert_eq!(walk(&c).last(), Some(&(Invalid, 0)));
}

#[test]
fn sort_matches_a_reference_and_the_heap_pops_lowest_priority_first() {
    let mut rng = Pcg32::new(9);
    for len in [0usize, 1, 5, 16, 17, 40, 300] {
        let mut keys: Vec<(u32, usize)> = (0..len).map(|i| (rng.below(6), i)).collect();
        let mut sorted = keys.clone();
        stl::sort(&mut sorted, &|a: &(u32, usize), b: &(u32, usize)| a.0 > b.0);
        assert!(sorted.windows(2).all(|w| w[0].0 >= w[1].0), "len {len}");
        let mut heap: Vec<(u32, usize)> = Vec::new();
        for k in keys.drain(..) {
            heap.push(k);
            stl::push_heap(&mut heap, &|a: &(u32, usize), b: &(u32, usize)| a.0 > b.0);
        }
        let mut last = 0;
        while !heap.is_empty() {
            stl::pop_heap(&mut heap, &|a: &(u32, usize), b: &(u32, usize)| a.0 > b.0);
            let top = heap.pop().unwrap();
            assert!(top.0 >= last, "len {len}");
            last = top.0;
        }
    }
}

#[test]
fn scaled_gains_index_the_table_by_ranks_gained_and_purchase_points_stop_at_zero() {
    let step = |threshold| RankStep {
        threshold,
        purchase_points_granted: 0,
    };
    let mut s = SkillTracker {
        points: 0.0,
        purchase_points: 3,
        next_rank: 2,
        ranks: vec![step(1.0), step(2.0), step(3.0), step(4.0), step(1000.0)],
        gain_scale: 1.0,
        base_rank: 2,
        scaling: vec![1.0, 0.5],
    };
    s.add(10.0, true);
    s.next_rank = 4;
    s.add(10.0, true);
    s.add(10.0, false);
    assert_eq!(s.points, 10.0 + 5.0 + 10.0);
    s.add_purchase_points(-9);
    assert_eq!(s.purchase_points, 0);
}

#[test]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn a_rounds_batch_is_sorted_again_after_every_roster() {
    let mut t = Tables::default();
    weapon(&mut t, "w", 0, false, &[]);
    body(&mut t, "b", 10.0, false);
    template(&mut t, "u", Soldier, "w", "x", "b");
    let level = |id: usize| ((id * 7) % 4) as i32 + 1;
    let member = |id: usize| Member {
        level: level(id),
        experience: id as f32,
        ..Member::new("u", 1)
    };
    let mut p = side(0, &[]);
    p.initial_units = (0..10).map(member).collect();
    p.armies = vec![Army {
        id: 1,
        named: false,
        members: (10..20).map(member).collect(),
    }];
    let ps = [p, side(1, &["u"])];

    let less = |a: &usize, b: &usize| level(*a) > level(*b);
    let mut batch: Vec<usize> = (0..10).collect();
    stl::sort(&mut batch, &less);
    batch.extend(10..20);
    stl::sort(&mut batch, &less);
    let mut once: Vec<usize> = (0..20).collect();
    stl::sort(&mut once, &less);
    assert_ne!(batch, once, "fixture must tell the two orders apart");

    let mut rng = Pcg32::new(1);
    let b = Battle::new(&ps, &t, TUTORIAL, &mut rng);
    let order: Vec<usize> = b.sides[0]
        .iter()
        .map(|&u| b.units[u].member.experience as usize)
        .collect();
    assert_eq!(order, batch);
}

#[test]
fn the_outcome_carries_both_sides_as_the_battle_opens() {
    let t = soldier_vs_archer();
    let ps = [side(0, &["soldier"]), side(1, &["archer"])];
    let out = resolve(&ps, &t, TUTORIAL, &mut Pcg32::new(1));
    assert_eq!(out.opening.round, 0);
    assert_eq!(out.opening.units, [1, 1]);
    assert_eq!(out.opening.hit_points, [1650.0, 1500.0]);
    assert!(out.log.first().is_some_and(|r| r.hit_points[0] < 1650.0));
}

fn side_order(battle: &Battle<'_>) -> Vec<usize> {
    battle.sides[0]
        .iter()
        .map(|&u| battle.units[u].member.object.map_or(0, |o| o as usize))
        .collect()
}

#[test]
fn an_army_with_no_members_still_re_sorts_the_sides_batch() {
    let t = soldier_vs_archer();
    let mut plain = side(0, &[]);
    plain.initial_units = (0..40_u32)
        .map(|i| {
            let mut member = Member::new("soldier", 1);
            member.level = i32::try_from((i * 7) % 3).unwrap();
            member.object = Some(i);
            member
        })
        .collect();
    let mut with_empty = plain.clone();
    with_empty.armies.push(Army {
        id: 1,
        named: false,
        members: Vec::new(),
    });
    let foe = side(1, &["archer"]);
    let order = |p: Participant| {
        let ps = [p, foe.clone()];
        let mut rng = Pcg32::new(1);
        side_order(&Battle::new(&ps, &t, TUTORIAL, &mut rng))
    };
    assert_ne!(order(plain), order(with_empty));
}

#[test]
fn a_round_cap_stops_a_battle_before_a_side_is_out() {
    let t = soldier_vs_archer();
    let ps = [side(0, &["soldier"]), side(1, &["archer"])];
    let capped = Options {
        round_cap: Some(2),
        ..TUTORIAL
    };
    assert_eq!(resolve(&ps, &t, capped, &mut Pcg32::new(1)).rounds, 2);
    assert!(resolve(&ps, &t, TUTORIAL, &mut Pcg32::new(1)).rounds > 2);
}
