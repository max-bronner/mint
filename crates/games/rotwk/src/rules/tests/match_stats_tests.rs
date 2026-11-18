use super::experience_tests::{dead, load, placed, rules, run};
use super::*;
use crate::game_data::ScoreWeights;
use crate::match_stats::{MatchStats, SAMPLE_PERIOD};

fn stats_of(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> MatchStats {
    let entity =
        crate::economy::player_entity(simulation.state(), RotwkRules::economy_ids(ids), player)
            .unwrap();
    simulation
        .state()
        .get(ids.match_stats, entity)
        .unwrap()
        .clone()
}

#[test]
fn a_kill_counts_for_the_killer_and_a_loss_for_the_victim_and_the_sample_carries_the_score() {
    let m = load();
    let weights = ScoreWeights {
        units_destroyed: 5.0,
        units_built: 7.0,
        ..ScoreWeights::default()
    };
    let rules = rules(
        &m,
        &[
            ("Frenzied", 200.0, 200.0, PLAYER),
            ("Target", 230.0, 200.0, OTHER),
        ],
    )
    .with_score_weights(weights);
    let mut simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let targets = placed(&simulation, ids, &m, "Target");
    for _ in 0..300 {
        run(&mut simulation, 1);
        if dead(&simulation, ids, &targets) == 1 {
            break;
        }
    }
    assert_eq!(dead(&simulation, ids, &targets), 1, "the target dies");
    run(&mut simulation, 2 * SAMPLE_PERIOD);

    let killer = stats_of(&simulation, ids, PLAYER);
    let victim = stats_of(&simulation, ids, OTHER);
    assert_eq!(killer.units_killed(), 1);
    assert_eq!(killer.units_lost, 0);
    assert_eq!(victim.units_lost, 1);
    assert_eq!(victim.units_killed(), 0);
    let last = |stats: &MatchStats| stats.samples().last().unwrap().score;
    assert_eq!(last(&killer), 5.0, "one kill at UnitsDestroyedMultiplier 5");
    assert_eq!(last(&victim), 0.0);
}

#[test]
fn a_players_own_index_never_scores() {
    let own = PlayerId::new(3);
    let weights = ScoreWeights {
        units_destroyed: 10.0,
        structures_destroyed: 100.0,
        ..ScoreWeights::default()
    };
    let mut stats = MatchStats::default();
    stats.add_kill(own, own, false);
    stats.add_kill(own, own, true);
    stats.add_kill(own, PlayerId::new(1), false);
    stats.add_kill(own, PlayerId::new(2), true);
    assert_eq!(stats.units_killed(), 1);
    assert_eq!(stats.structures_destroyed(), 1);
    assert_eq!(stats.score(own, &weights), 110.0);
}

#[test]
fn spending_counts_in_the_total_and_a_refund_only_lowers_its_category() {
    let mut stats = MatchStats::default();
    stats.spend(crate::match_stats::SpendKind::Units, 100);
    stats.refund(crate::match_stats::SpendKind::Units, 60);
    assert_eq!((stats.spent(), stats.spent_on_units), (100, 40));
}

#[test]
fn the_score_adds_the_vetted_power_skill_and_elimination_terms_after_the_kills() {
    let weights = ScoreWeights {
        skill_points: 2.0,
        power_points: 3.0,
        heroes_vetted: 5.0,
        units_vetted: 7.0,
        ..ScoreWeights::default()
    };
    let mut stats = MatchStats::default();
    stats.skill_points = 10.0;
    stats.power_points = 1;
    stats.heroes_vetted = 1;
    stats.units_vetted = 1;
    stats.elimination_bonus = 100.0;
    assert_eq!(
        stats.score(PlayerId::new(0), &weights),
        20.0 + 3.0 + 5.0 + 7.0 + 100.0
    );
}

#[test]
fn a_kill_of_a_non_scoring_structure_counts_in_the_killers_kills_by_type() {
    let m = load();
    let rules = rules(
        &m,
        &[
            ("Frenzied", 200.0, 200.0, PLAYER),
            ("Barracks", 600.0, 600.0, OTHER),
        ],
    )
    .with_objects_that_score(crate::defeat::ObjectFilter::parse(&[
        "ANY".to_owned(),
        "+SCORE".to_owned(),
    ]));
    let mut simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let kind = |name: &str| simulation.rules().kinds.kind(name).unwrap().0;
    let (barracks, frenzied) = (kind("Barracks"), kind("Frenzied"));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::DebugCommand::KillBy {
                target: crate::rules::DebugTarget::Kind {
                    kind: barracks,
                    side: None,
                },
                killer: crate::rules::DebugTarget::Kind {
                    kind: frenzied,
                    side: None,
                },
            }),
        )
        .unwrap();
    run(&mut simulation, 2);
    assert_eq!(stats_of(&simulation, ids, PLAYER).kills_of(barracks), 1);
}
