use super::*;

use crate::model::{BattleMode, Member, PlayerCarry, PlotBuilding};

fn spec_of(events: &[Event]) -> BattleSpec {
    events
        .iter()
        .find_map(|e| match e {
            Event::BattleSpec(spec) => Some(spec.clone()),
            _ => None,
        })
        .unwrap()
}

fn real_time(s: &mut LivingWorldState, region: RegionId) -> BattleSpec {
    let events = command(
        s,
        P0,
        Command::ResolveBattle {
            region,
            mode: BattleMode::RealTime,
        },
    );
    spec_of(&events)
}

#[test]
fn a_real_time_spec_names_sides_cash_reinforcements_and_plot_structures() {
    let mut s = session();
    let a = rid(&s, "A");
    let keep = s.rules.building_id("Keep").unwrap();
    s.regions[a.index()].plots[0] = Some(PlotBuilding::new(keep, 0, false));
    let first = hero_army(&mut s, P1, "B", 1);
    let second = hero_army(&mut s, P1, "B", 2);
    for army in [first, second] {
        command(&mut s, P1, Command::MoveArmy { army, dest: a });
    }
    end_planning(&mut s);
    let spec = real_time(&mut s, a);

    let summary: Vec<_> = spec
        .participants
        .iter()
        .map(|p| {
            let arrivals: Vec<_> = p
                .armies
                .iter()
                .map(|x| (x.hero.is_some(), x.arrives_after, x.primary))
                .collect();
            let plots: Vec<_> = p
                .structures
                .iter()
                .map(|x| (x.plot, x.template.clone()))
                .collect();
            (p.side_name.clone(), p.defender, p.cash, arrivals, plots)
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                "Player_1".to_owned(),
                true,
                2000,
                vec![(false, None, true)],
                vec![(1, "KeepBattle".to_owned())],
            ),
            (
                "Player_3".to_owned(),
                false,
                1000,
                vec![(true, None, true), (true, Some(300), false)],
                Vec::new(),
            ),
        ]
    );
}

fn orc(s: &LivingWorldState) -> crate::model::UnitId {
    s.rules.unit_id("orc").unwrap()
}

fn orcs(s: &mut LivingWorldState, n: u32) -> Vec<Member> {
    let unit = orc(s);
    (0..n)
        .map(|_| Member {
            id: s.fresh_member_id(),
            unit,
            level: 1,
            count: 1,
            experience: 0,
            battles: 0,
            kills: 0,
            kills_battle: 0,
            upgraded_at: None,
        })
        .collect()
}

fn defended_e() -> (LivingWorldState, ArmyId, ArmyId, ArmyId) {
    let mut s = session();
    let attacker = hero_army(&mut s, P0, "D", 1);
    let led = hero_army(&mut s, P1, "E", 2);
    let e = rid(&s, "E");
    let garrison = s.spawn_army(P1, e, None, &[("orc".to_owned(), 1)]);
    command(
        &mut s,
        P0,
        Command::MoveArmy {
            army: attacker,
            dest: e,
        },
    );
    end_planning(&mut s);
    real_time(&mut s, e);
    (s, attacker, led, garrison)
}

fn finish(s: &mut LivingWorldState, rosters: Vec<(ArmyId, Vec<Member>)>) -> Vec<Event> {
    let region = rid(s, "E");
    command(
        s,
        P0,
        Command::ApplyBattleResult {
            region,
            result: BattleResult {
                winner: Some(1),
                rosters,
                fallen_heroes: Vec::new(),
                fallen_records: Vec::new(),
                players: Vec::new(),
            },
        },
    )
}

fn hop() -> (u64, u64) {
    let (mut s, attacker, led, garrison) = defended_e();
    let held = s.state_hash();
    let hero = s.armies[&led].roster[0].clone();
    let survivors = orcs(&mut s, 4);
    finish(
        &mut s,
        vec![
            (attacker, Vec::new()),
            (led, vec![hero]),
            (garrison, survivors),
        ],
    );
    (held, s.state_hash())
}

#[test]
fn the_same_session_and_battle_give_the_same_strategic_hashes_after_the_hop() {
    assert_eq!(hop(), hop());
}

#[test]
fn every_unit_kept_at_a_price_is_named_with_that_price() {
    let (mut s, attacker, led, garrison) = defended_e();
    let hero = s.armies[&led].roster[0].clone();
    let survivors = orcs(&mut s, 4);
    let events = finish(
        &mut s,
        vec![
            (attacker, Vec::new()),
            (led, vec![hero]),
            (garrison, survivors),
        ],
    );
    let named: Vec<(ArmyId, i32)> = events
        .iter()
        .filter_map(|e| match e {
            Event::MemberCharged {
                army,
                member,
                price,
            } => s.armies[army]
                .roster
                .iter()
                .any(|m| m.id == *member)
                .then_some((*army, *price)),
            _ => None,
        })
        .collect();
    assert_eq!(named.iter().map(|(_, price)| price).sum::<i32>(), 10);
}

#[test]
fn a_winning_garrison_refills_its_hero_army_free_and_pays_for_units_built_in_battle() {
    let (mut s, attacker, led, garrison) = defended_e();
    let hero = s.armies[&led].roster[0].clone();
    let survivors = orcs(&mut s, 4);
    let events = finish(
        &mut s,
        vec![
            (attacker, Vec::new()),
            (led, vec![hero]),
            (garrison, survivors),
        ],
    );
    assert!(events.contains(&Event::GarrisonKept {
        army: garrison,
        charged: 10
    }));
    let counts = (
        count_units(&s, led),
        count_units(&s, garrison),
        s.players[1].resources,
    );
    assert_eq!(counts, (3, 2, 90));
}

#[test]
fn a_losing_hero_army_that_reached_the_field_keeps_only_its_hero() {
    let (mut s, attacker, led, garrison) = defended_e();
    let survivors = s.armies[&attacker].roster.clone();
    let kept = s.armies[&led].roster.clone();
    finish(
        &mut s,
        vec![(attacker, survivors), (led, kept), (garrison, Vec::new())],
    );
    let left: Vec<bool> = s.armies[&attacker]
        .roster
        .iter()
        .map(|m| s.rules.unit(m.unit).hero)
        .collect();
    assert_eq!(left, vec![true]);
}

fn count_units(s: &LivingWorldState, army: ArmyId) -> u32 {
    s.armies[&army].roster.iter().map(|m| m.count).sum()
}

#[test]
fn a_unit_takes_its_stacks_battles_and_kills_into_the_next_battle() {
    let s = session();
    let mut before = orcs(&mut s.clone(), 1);
    before[0].count = 2;
    before[0].battles = 3;
    before[0].kills = 5;
    let mut after = orcs(&mut s.clone(), 3);
    for member in &mut after {
        member.kills_battle = 2;
    }
    let carried = crate::battle::with_history(&before, &after);
    let seen: Vec<_> = carried
        .iter()
        .map(|m| (m.battles, m.kills, m.kills_battle))
        .collect();
    assert_eq!(seen, vec![(4, 7, 2), (4, 7, 2), (1, 2, 2)]);
}

fn defended_e_seed(troops: u32) -> u64 {
    let mut s = session();
    let attacker = hero_army(&mut s, P0, "D", troops);
    let e = rid(&s, "E");
    command(
        &mut s,
        P0,
        Command::MoveArmy {
            army: attacker,
            dest: e,
        },
    );
    end_planning(&mut s);
    real_time(&mut s, e).seed
}

#[test]
fn a_battles_seed_follows_the_strategic_state_and_repeats_for_the_same_session() {
    let seed = defended_e_seed(1);
    assert_eq!(seed, defended_e_seed(1));
    assert_ne!(seed, defended_e_seed(2));
}

#[test]
fn a_human_losing_a_lose_campaign_region_ends_the_session_and_elsewhere_it_goes_on() {
    let ended = |flag: bool| {
        let (mut s, attacker, led, garrison) = defended_e();
        let e = rid(&s, "E");
        s.rules.regions[e.index()].lose_campaign_if_lose_map = flag;
        let roster = |s: &LivingWorldState, a: ArmyId| s.armies[&a].roster.clone();
        let rosters = vec![
            (attacker, roster(&s, attacker)),
            (led, roster(&s, led)),
            (garrison, roster(&s, garrison)),
        ];
        finish(&mut s, rosters);
        s.ended
    };
    assert_eq!((ended(true), ended(false)), (Some(None), None));
}

#[test]
fn a_participants_tactical_cp_is_the_region_limit_plus_hero_cp_for_each_hero_army_in_it() {
    let mut s = session();
    place_done(&mut s, "A", 0, "Keep");
    let a = rid(&s, "A");
    for troops in [1, 2] {
        let army = hero_army(&mut s, P1, "B", troops);
        command(&mut s, P1, Command::MoveArmy { army, dest: a });
    }
    end_planning(&mut s);
    let spec = real_time(&mut s, a);
    let limits: Vec<i32> = spec.participants.iter().map(|p| p.command_points).collect();
    assert_eq!(limits, vec![3, 15]);
}

#[test]
fn a_participant_carries_its_players_rank_state_into_the_spec() {
    let mut s = session();
    s.players[0].skill_points = 9.0;
    s.players[0].sciences = vec!["SCIENCE_MEN".to_owned()];
    s.players[0].upgrades = vec!["Upgrade_Persists".to_owned()];
    place_done(&mut s, "A", 0, "Keep");
    let a = rid(&s, "A");
    let army = hero_army(&mut s, P1, "B", 1);
    command(&mut s, P1, Command::MoveArmy { army, dest: a });
    end_planning(&mut s);
    let spec = real_time(&mut s, a);
    let carried = &spec.participants[0];
    assert_eq!(
        (&carried.skill_points, &carried.sciences, &carried.upgrades),
        (
            &9.0,
            &vec!["SCIENCE_MEN".to_owned()],
            &vec!["Upgrade_Persists".to_owned()]
        )
    );
}

#[test]
fn a_real_time_result_stores_each_players_rank_state_for_the_next_battle() {
    let (mut s, attacker, led, garrison) = defended_e();
    let region = rid(&s, "E");
    let carry = PlayerCarry {
        skill_points: 14.0,
        rank_points: 6,
        sciences: vec!["SCIENCE_ORCS".to_owned()],
        upgrades: vec!["Upgrade_Persists".to_owned()],
    };
    command(
        &mut s,
        P0,
        Command::ApplyBattleResult {
            region,
            result: BattleResult {
                winner: Some(1),
                rosters: vec![
                    (attacker, Vec::new()),
                    (led, Vec::new()),
                    (garrison, Vec::new()),
                ],
                players: vec![(P1, carry.clone())],
                ..BattleResult::default()
            },
        },
    );
    let held = &s.players[1];
    assert_eq!(
        (
            held.skill_points,
            held.rank_points,
            &held.sciences,
            &held.upgrades
        ),
        (14.0, 6, &carry.sciences, &carry.upgrades)
    );
}

#[test]
fn a_winning_garrison_is_kept_in_order_of_experience_not_level() {
    let (mut s, attacker, led, garrison) = defended_e();
    let kept = s.armies[&led].roster.clone();
    let mut survivors = orcs(&mut s, 3);
    for (member, experience) in survivors.iter_mut().zip([50, 5, 20]) {
        member.experience = experience;
    }
    finish(
        &mut s,
        vec![(attacker, Vec::new()), (led, kept), (garrison, survivors)],
    );
    let order: Vec<u32> = s.armies[&garrison]
        .roster
        .iter()
        .map(|m| m.experience)
        .collect();
    assert_eq!(order, vec![5, 20, 50]);
}

#[test]
fn fresh_rank_grants_sum_the_ranks_the_whole_points_reach() {
    let (mut s, ..) = defended_e();
    s.rules.ranks = [(0.0, 5), (60.0, 2), (120.0, 3)]
        .into_iter()
        .map(|(threshold, granted)| crate::rules::RankGrant { threshold, granted })
        .collect();
    assert_eq!(s.rules.fresh_rank_grants(59.9), 5);
    assert_eq!(s.rules.fresh_rank_grants(90.0), 7);
    assert_eq!(s.rules.fresh_rank_grants(5000.0), 10);
    assert_eq!(s.rules.fresh_rank_grants(0.0), 0);
}
