#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::needless_pass_by_value
)]

use mint_sage_autoresolve::roster::{Arrival, Outcome, ParticipantRound, RoundLog};

use super::*;

const ROUNDS: u32 = 25;

fn field(f: impl Fn(u32) -> f32) -> Vec<ParticipantRound> {
    (0..=ROUNDS)
        .map(|r| {
            let hit_points = f(r).max(0.0);
            ParticipantRound {
                units: usize::from(hit_points > 0.0),
                hit_points,
            }
        })
        .collect()
}

fn arrival(army: u32, round: u32, hit_points: f32) -> Arrival {
    Arrival {
        army,
        round,
        hit_points,
    }
}

fn battle() -> ResolvedBattle {
    let p0 = field(|r| 100.0 - 2.0 * f32::from(u16::try_from(r).unwrap()));
    let p1 = field(|r| {
        let r = f32::from(u16::try_from(r).unwrap());
        if r < 15.0 {
            100.0
        } else {
            200.0 - 2.0 * (r - 15.0)
        }
    });
    let p2 = field(|r| 150.0 - 6.0 * f32::from(u16::try_from(r).unwrap()));
    let side_hp = |r: usize, a: &[ParticipantRound], b: &[ParticipantRound]| {
        (a[r].hit_points + b[r].hit_points, a[r].units + b[r].units)
    };
    let log_at = |r: usize| {
        let (hp0, u0) = side_hp(r, &p0, &p1);
        RoundLog {
            round: u32::try_from(r).unwrap(),
            units: [u0, p2[r].units],
            hit_points: [hp0, p2[r].hit_points],
        }
    };
    let outcome = Outcome {
        winner: 0,
        rounds: ROUNDS,
        participants: Vec::new(),
        destroyed_objects: Vec::new(),
        fallen_heroes: Vec::new(),
        kills: Vec::new(),
        log: (1..=ROUNDS as usize).map(log_at).collect(),
        opening: log_at(0),
        fields: vec![p0, p1, p2],
        arrivals: vec![
            vec![arrival(10, 0, 100.0)],
            vec![arrival(11, 0, 100.0), arrival(12, 15, 100.0)],
            vec![arrival(20, 0, 150.0)],
        ],
        draws: 0,
    };
    ResolvedBattle {
        region: mint_sage_livingworld::RegionId(0),
        battle: 7,
        sides: [vec![PlayerId(0), PlayerId(1)], vec![PlayerId(2)]],
        order: vec![PlayerId(0), PlayerId(1), PlayerId(2)],
        outcome,
        heroes: [(11, "Aragorn".to_owned()), (12, "Theoden".to_owned())]
            .into_iter()
            .collect(),
    }
}

fn start_loaded(plan: Plan, step: u32) -> (Playback, Vec<Out>) {
    let slots: Vec<Slot> = plan.entries.iter().map(|e| e.slot).collect();
    let (mut playback, mut out) = Playback::start(plan, step);
    for slot in slots {
        out.extend(playback.panel_loaded(slot.panel, slot.index));
    }
    (playback, out)
}

fn play(plan: Plan, step: u32) -> Vec<Out> {
    let (mut playback, first) = start_loaded(plan, step);
    let mut log = first.clone();
    let mut pending: VecDeque<Out> = first.into();
    while let Some(out) = pending.pop_front() {
        let events: &[Event] = match out {
            Out::Call(Call::Reinforce(..)) => &[Event::ReinforceDone],
            Out::Call(Call::Hit(..)) => &[Event::Chain, Event::HitDone],
            _ => &[],
        };
        for event in events {
            let more = playback.event(*event);
            log.extend(more.iter().cloned());
            pending.extend(more);
        }
    }
    assert!(playback.finished(), "the dialog ran to its finish");
    log
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn calls(log: &[Out]) -> Vec<Call> {
    log.iter()
        .filter_map(|o| match o {
            Out::Call(Call::Health(s, p)) => Some(Call::Health(*s, round2(*p))),
            Out::Call(Call::Hit(s, p)) => Some(Call::Hit(*s, round2(*p))),
            Out::Call(Call::Reinforce(s, p)) => Some(Call::Reinforce(*s, round2(*p))),
            Out::Call(c) => Some(c.clone()),
            Out::Post(_) => None,
        })
        .collect()
}

fn ally(index: u32) -> Slot {
    Slot {
        panel: Panel::Allies,
        index,
    }
}

fn enemy(index: u32) -> Slot {
    Slot {
        panel: Panel::Enemies,
        index,
    }
}

#[test]
fn there_is_one_panel_per_participant_split_into_allies_and_the_rest() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let panels: Vec<(PlayerId, Slot)> = plan.entries.iter().map(|e| (e.player, e.slot)).collect();
    assert_eq!(
        panels,
        vec![
            (PlayerId(0), ally(0)),
            (PlayerId(1), ally(1)),
            (PlayerId(2), enemy(0))
        ]
    );
    assert_eq!(
        (plan.count(Panel::Allies), plan.count(Panel::Enemies)),
        (2, 1)
    );
    let from_the_enemy = super::plan(&battle(), PlayerId(2)).unwrap();
    assert_eq!(from_the_enemy.entries[0].player, PlayerId(2));
    assert!(!from_the_enemy.victory);
    assert!(plan.victory);
}

#[test]
fn every_bar_is_a_percent_of_the_largest_total_including_batches_still_to_come() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    assert_eq!(plan.whole, 200.0);
    let (_, open) = start_loaded(plan, 10);
    let health: Vec<Call> = calls(&open)
        .into_iter()
        .filter(|c| matches!(c, Call::Health(..)))
        .collect();
    assert_eq!(
        health,
        vec![
            Call::Health(ally(0), 50.0),
            Call::Health(ally(1), 50.0),
            Call::Health(enemy(0), 75.0),
        ]
    );
}

#[test]
fn the_dialog_steps_battle_rounds_per_step_posting_one_message_per_step() {
    let log = play(plan(&battle(), PlayerId(0)).unwrap(), 10);
    let posts: Vec<Post> = log
        .iter()
        .filter_map(|o| match o {
            Out::Post(p) => Some(*p),
            Out::Call(_) => None,
        })
        .collect();
    assert_eq!(
        posts,
        vec![
            Post::Step { target: 10 },
            Post::Step { target: 20 },
            Post::Step { target: 30 }
        ]
    );
    let log = play(plan(&battle(), PlayerId(0)).unwrap(), 5);
    let steps = log
        .iter()
        .filter(|o| matches!(o, Out::Post(Post::Step { .. })))
        .count();
    assert_eq!(steps, 5, "targets 5, 10, 15, 20 and 25");
}

#[test]
fn within_a_step_reinforcements_play_together_first_then_hits_one_panel_at_a_time_enemies_first() {
    let log = calls(&play(plan(&battle(), PlayerId(0)).unwrap(), 10));
    let motion: Vec<&Call> = log
        .iter()
        .filter(|c| matches!(c, Call::Hit(..) | Call::Reinforce(..)))
        .collect();
    assert_eq!(
        motion,
        vec![
            &Call::Hit(enemy(0), 30.0),
            &Call::Hit(ally(0), 10.0),
            &Call::Reinforce(ally(1), 45.0),
            &Call::Hit(enemy(0), 30.0),
            &Call::Hit(ally(0), 10.0),
            &Call::Hit(enemy(0), 15.0),
            &Call::Hit(ally(0), 5.0),
            &Call::Hit(ally(1), 5.0),
        ]
    );
}

#[test]
fn a_reinforcement_cannot_fill_the_bar_past_full_and_a_hit_cannot_empty_it_past_zero() {
    let mut big = battle();
    for round in 15..=ROUNDS as usize {
        big.outcome.fields[1][round].hit_points = 400.0;
    }
    let log = calls(&play(plan(&big, PlayerId(0)).unwrap(), 10));
    assert!(
        log.contains(&Call::Reinforce(ally(1), 50.0)),
        "300 hit points would be 150% of a bar already at 50%: only the 50% left fit"
    );
}

#[test]
fn a_hit_waits_for_the_chain_event_before_the_next_panel_is_hit() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let (mut playback, open) = start_loaded(plan, 10);
    let hits = |outs: &[Out]| {
        outs.iter()
            .filter(|o| matches!(o, Out::Call(Call::Hit(..))))
            .count()
    };
    assert_eq!(hits(&open), 1, "only the first panel is hit");
    assert_eq!(hits(&playback.event(Event::Chain)), 1);
    assert_eq!(
        hits(&playback.event(Event::Chain)),
        0,
        "nothing left in the step"
    );
}

#[test]
fn player_health_is_sent_on_load_a_rise_without_reinforcement_and_the_finish_only() {
    let log = calls(&play(plan(&battle(), PlayerId(0)).unwrap(), 10));
    let health: Vec<&Call> = log
        .iter()
        .filter(|c| matches!(c, Call::Health(..)))
        .collect();
    assert_eq!(
        health,
        vec![
            &Call::Health(ally(0), 50.0),
            &Call::Health(ally(1), 50.0),
            &Call::Health(enemy(0), 75.0),
            &Call::Health(ally(0), 25.0),
            &Call::Health(ally(1), 90.0),
            &Call::Health(enemy(0), 0.0),
        ],
        "three at load, three at the finish, none per step"
    );

    let mut regenerating = battle();
    regenerating.outcome.fields[0][12].hit_points = 120.0;
    regenerating.outcome.fields[0][10].hit_points = 120.0;
    let log = calls(&play(plan(&regenerating, PlayerId(0)).unwrap(), 10));
    assert!(
        log.contains(&Call::Health(ally(0), 60.0)),
        "a rise with no batch is set at once"
    );
}

#[test]
fn the_finish_sends_each_panel_health_and_state_allies_first_then_the_verdict_then_the_fade() {
    let log = calls(&play(plan(&battle(), PlayerId(0)).unwrap(), 10));
    let at = log
        .iter()
        .position(|c| *c == Call::Health(ally(0), 25.0))
        .unwrap();
    assert_eq!(
        log[at..].to_vec(),
        vec![
            Call::Health(ally(0), 25.0),
            Call::State(ally(0), SURVIVED),
            Call::Health(ally(1), 90.0),
            Call::State(ally(1), SURVIVED),
            Call::Health(enemy(0), 0.0),
            Call::Result(WIN),
            Call::FadeOut,
        ],
        "the enemy's `_eliminated` went out when its last hit finished, and is not sent twice"
    );
    let eliminated = log
        .iter()
        .filter(|c| **c == Call::State(enemy(0), ELIMINATED))
        .count();
    assert_eq!(eliminated, 1);
    let first_state = log
        .iter()
        .position(|c| matches!(c, Call::State(..)))
        .unwrap();
    assert_eq!(log[first_state], Call::State(enemy(0), ELIMINATED));
}

#[test]
fn the_verdict_is_win_or_lose_for_the_first_ally_panel() {
    let log = calls(&play(plan(&battle(), PlayerId(2)).unwrap(), 10));
    assert!(log.contains(&Call::Result(LOSE)));
    assert!(!log.contains(&Call::Result(WIN)));
    assert_eq!((WIN, LOSE), ("_win", "_lose"));
}

#[test]
fn a_reinforcement_batch_brings_the_next_banner() {
    let log = calls(&play(plan(&battle(), PlayerId(0)).unwrap(), 10));
    let banners: Vec<&Call> = log
        .iter()
        .filter(|c| matches!(c, Call::Banner(..)))
        .collect();
    assert_eq!(
        banners,
        vec![
            &Call::Banner(ally(0), 0, true),
            &Call::Banner(ally(1), 0, true),
            &Call::Banner(enemy(0), 0, true),
            &Call::Banner(ally(1), 1, true),
        ]
    );
}

#[test]
fn the_start_sends_only_the_counts_and_the_first_step_and_each_panel_fills_on_its_own_load() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let (mut playback, open) = Playback::start(plan, 10);
    assert_eq!(
        open,
        vec![
            Out::Call(Call::NumPlayers(Panel::Allies, 2)),
            Out::Call(Call::NumPlayers(Panel::Enemies, 1)),
            Out::Post(Post::Step { target: 10 }),
        ]
    );
    let first = playback.panel_loaded(Panel::Allies, 1);
    assert_eq!(
        calls(&first),
        vec![
            Call::Identity(1),
            Call::Health(ally(1), 50.0),
            Call::UnitCount(ally(1), 1),
            Call::Banner(ally(1), 0, true),
        ],
        "only that panel, and no step is read while others are missing"
    );
    assert!(playback
        .panel_loaded(Panel::Enemies, 0)
        .iter()
        .all(|o| !matches!(
            o,
            Out::Call(Call::Hit(..) | Call::Reinforce(..)) | Out::Post(_)
        )));
    let last = playback.panel_loaded(Panel::Allies, 0);
    assert!(
        last.iter().any(|o| matches!(o, Out::Call(Call::Hit(..)))),
        "the last panel to load starts the first step"
    );
    assert!(
        playback.panel_loaded(Panel::Allies, 7).is_empty(),
        "no such panel"
    );
}

#[test]
fn a_panel_that_loads_again_is_filled_again_without_restarting_the_step() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let (mut playback, _) = start_loaded(plan, 10);
    let again = calls(&playback.panel_loaded(Panel::Enemies, 0));
    assert_eq!(again[0], Call::Identity(2));
    assert!(again
        .iter()
        .any(|c| matches!(c, Call::Health(s, _) if *s == enemy(0))));
    assert!(!again.iter().any(|c| matches!(c, Call::Hit(..))));
}

#[test]
fn a_banner_asks_for_the_hero_of_the_army_in_its_place_among_those_that_enter() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let ally1 = &plan.entries[1];
    assert_eq!(ally1.banner_hero(0), Some("Aragorn"));
    assert_eq!(ally1.banner_hero(1), Some("Theoden"));
    assert_eq!(ally1.banner_hero(2), None);
    assert_eq!(
        plan.entries[0].banner_hero(0),
        None,
        "a garrison has no hero"
    );
}

#[test]
fn a_skip_posts_the_skip_message_and_runs_the_finish_with_no_animation() {
    let plan = plan(&battle(), PlayerId(0)).unwrap();
    let (mut playback, _) = start_loaded(plan, 10);
    let out = playback.skip();
    assert_eq!(out[0], Out::Post(Post::Skip));
    let after = calls(&out);
    assert!(!after
        .iter()
        .any(|c| matches!(c, Call::Hit(..) | Call::Reinforce(..))));
    assert!(after.ends_with(&[Call::Result(WIN), Call::FadeOut]));
    assert!(after.contains(&Call::Health(ally(0), 25.0)));
    assert!(playback.finished());
    assert!(playback.skip().is_empty(), "skipping twice does nothing");
    assert!(playback.event(Event::HitDone).is_empty());
}
