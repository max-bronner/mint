use mint_math::rng::Pcg32;

use super::{rid, session, Verdict, P0, P1};
use crate::ai::catalogue::Catalogue;
use crate::ai::data::HERO;
use crate::ai::hero::{capital_defence, outlook};
use crate::ai::recruit;
use crate::ai::retreat::{battle_answer, retreat_choice};
use crate::ai::snapshot::{composition_weight, Snapshot};
use crate::ai::turn::execute;
use crate::ai::turn::Skipped;
use crate::ai::{BattleAnswer, Ctx, Predictor, Roll, Rolls, Scale};
use crate::ai::{MoveKind, MoveOrder, StrategicAi};
use crate::model::{PlayerId, PlayerKind};
use crate::state::LivingWorldState;

fn with_ctx<R>(
    state: &LivingWorldState,
    player: PlayerId,
    difficulty: u8,
    predictor: &mut dyn Predictor,
    run: impl FnOnce(&mut Ctx<'_>) -> R,
) -> (R, Vec<Roll>) {
    let snap = Snapshot::take(state);
    let template = state.rules.ai.template(None);
    let catalogue = Catalogue::build(&state.rules, state.player(player).unwrap().template);
    let mut rolls = Rolls::new(state.rng, true);
    let out = {
        let mut ctx = Ctx {
            state,
            snap: &snap,
            catalogue: &catalogue,
            template: &template,
            me: player,
            difficulty,
            rolls: &mut rolls,
            predictor,
        };
        run(&mut ctx)
    };
    (out, rolls.take_log())
}

fn duel() -> LivingWorldState {
    let mut state = session(
        &[(PlayerKind::Human, "A"), (PlayerKind::Computer, "H")],
        8,
        "",
        1,
    );
    state.armies.clear();
    state
}

#[test]
fn int_always_draws_and_real_draws_only_over_a_range() {
    let mut rolls = Rolls::new(Pcg32::new(7), false);
    let start = *rolls.rng();
    rolls.int(0, 0);
    let after_int = *rolls.rng();
    assert_ne!(after_int, start, "int(0, 0) must consume a draw");
    assert_eq!(rolls.real(5.0, 5.0), 5.0);
    assert_eq!(rolls.real(5.0, 3.0), 3.0);
    assert_eq!(
        *rolls.rng(),
        after_int,
        "real without a range must not draw"
    );
    rolls.real(0.0, 4.0);
    assert_ne!(*rolls.rng(), after_int);
}

#[test]
fn composition_sums_member_ids_not_counts() {
    assert_eq!(composition_weight(&[(0, 3), (0, 4), (1, 9)], 0), 7);
}

#[test]
fn a_named_hero_weighs_ten_and_an_unnamed_one_weighs_one() {
    let state = duel();
    let mut verdict = Verdict::new(false, 1);
    let (_, log) = with_ctx(&state, P1, 3, &mut verdict, |ctx| {
        let barracks = ctx.catalogue.roles.barracks.unwrap();
        let spawner = ctx.catalogue.spawners.get(&barracks).unwrap();
        recruit::pick(ctx, barracks, spawner, HERO)
    });
    assert!(
        matches!(log.as_slice(), [Roll::Int { lo: 0, hi: 20, .. }]),
        "{log:?}"
    );
}

#[test]
fn the_ai_participant_is_scaled_by_difficulty() {
    let mut state = duel();
    for name in ["B", "D"] {
        let id = rid(&state, name);
        state.regions[id.index()].owner = Some(P1);
    }
    let (b, c, d) = (rid(&state, "B"), rid(&state, "C"), rid(&state, "D"));
    let hero = state.spawn_army(P1, b, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    state.spawn_army(P0, c, Some("hero_a"), &[("hero_a".to_owned(), 1)]);
    state.spawn_army(P1, d, None, &[("elf".to_owned(), 1)]);
    let scale = |weapon, armor| Some(Scale { weapon, armor });
    let table = [
        (2, [scale(0.75, 1.25), scale(0.5, 1.5)]),
        (3, [scale(0.9, 1.1), scale(0.9, 1.1)]),
        (4, [None, None]),
        (5, [None, None]),
    ];
    for (difficulty, expected) in table {
        let mut verdict = Verdict::new(false, 1);
        let (m, _) = with_ctx(&state, P1, difficulty, &mut verdict, |ctx| {
            outlook(ctx, hero, c)
        });
        assert_eq!(m, 2);
        let asked: Vec<Option<Scale>> = verdict.asked.iter().map(|(_, s)| *s).collect();
        assert_eq!(asked, expected, "difficulty {difficulty}");
    }
}

#[test]
fn a_retreat_goes_to_an_allied_neighbour_else_the_base() {
    let mut state = duel();
    let e = rid(&state, "E");
    state.regions[e.index()].owner = Some(P1);
    let (b, d) = (rid(&state, "B"), rid(&state, "D"));
    let near = state.spawn_army(P1, d, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    let lost = state.spawn_army(P1, b, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    let mut verdict = Verdict::new(false, 1);
    let ((to_ally, to_base), log) = with_ctx(&state, P1, 3, &mut verdict, |ctx| {
        (retreat_choice(ctx, near), retreat_choice(ctx, lost))
    });
    assert_eq!(to_ally, Some(e));
    assert_eq!(to_base, Some(rid(&state, "H")));
    assert_eq!(log.len(), 1, "no draw when the base is the only choice");
}

#[test]
fn a_battle_is_left_only_when_it_is_lost_in_under_six_rounds() {
    let mut state = duel();
    let e = rid(&state, "E");
    state.regions[e.index()].owner = Some(P1);
    let (b, d) = (rid(&state, "B"), rid(&state, "D"));
    for region in [b, d] {
        state.spawn_army(P1, region, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
        state.spawn_army(P0, region, Some("hero_a"), &[("hero_a".to_owned(), 1)]);
    }
    let mut heroless = state.clone();
    heroless
        .armies
        .retain(|_, a| !(a.player == P1 && a.region == d));
    heroless.spawn_army(P1, d, None, &[("elf".to_owned(), 1)]);
    let table = [
        (d, true, 5, BattleAnswer::Abstain),
        (d, true, 8, BattleAnswer::Abstain),
        (d, false, 5, BattleAnswer::Withdraw),
        (d, false, 7, BattleAnswer::Abstain),
        (b, false, 2, BattleAnswer::Abstain),
    ];
    for (region, win, rounds, expected) in table {
        let mut verdict = Verdict::new(win, rounds);
        let (answer, _) = with_ctx(&state, P1, 3, &mut verdict, |ctx| {
            battle_answer(ctx, region)
        });
        assert_eq!(answer, expected, "win {win} rounds {rounds}");
    }
    let mut verdict = Verdict::new(false, 2);
    let (answer, _) = with_ctx(&heroless, P1, 3, &mut verdict, |ctx| battle_answer(ctx, d));
    assert_eq!(answer, BattleAnswer::Abstain, "no hero army, no withdrawal");
}

fn mates() -> LivingWorldState {
    let mut state = session(
        &[
            (PlayerKind::Computer, "A"),
            (PlayerKind::Computer, "B"),
            (PlayerKind::Human, "H"),
        ],
        8,
        "",
        1,
    );
    state.armies.clear();
    state.players[1].team = state.players[0].team;
    for c in &mut state.rules.conditions.player_defeat {
        c.lose_if_capital_lost = true;
    }
    state
}

#[test]
fn capital_defence_names_a_teammates_hero_army() {
    let mut state = mates();
    let (b, c) = (rid(&state, "B"), rid(&state, "C"));
    let mate = state.spawn_army(P1, c, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    state.spawn_army(PlayerId(2), b, Some("hero_c"), &[("hero_c".to_owned(), 1)]);
    let mut verdict = Verdict::new(true, 3);
    let (moves, _) = with_ctx(&state, P0, 3, &mut verdict, |ctx| {
        let mut moves = Vec::new();
        capital_defence(ctx, &mut moves);
        moves
    });
    assert!(moves.iter().any(|m| m.army == mate));
}

#[test]
fn a_planned_move_of_an_army_the_seat_does_not_own_is_skipped_without_a_rejection() {
    let mut state = mates();
    let (c, d) = (rid(&state, "C"), rid(&state, "D"));
    let mate = state.spawn_army(P1, c, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    let mut ai = StrategicAi::for_computers(&state, 3, false);
    let seat = ai.seats.iter_mut().find(|s| s.player == P0).unwrap();
    seat.plan.moves.push(MoveOrder {
        army: mate,
        heroless: false,
        kind: MoveKind::Move { region: d },
    });
    let mut skipped = Skipped::default();
    let commands = execute(&state, seat, &mut skipped);
    assert!(commands.is_empty());
    assert_eq!(skipped.illegal_moves, 0);
}

#[test]
fn a_hero_the_player_already_has_is_not_recruitable_named_or_not() {
    let mut state = duel();
    let (a, b, c) = (rid(&state, "A"), rid(&state, "B"), rid(&state, "C"));
    state.spawn_army(P1, a, Some("hero_a"), &[("hero_a".to_owned(), 1)]);
    state.spawn_army(P1, b, Some("hero_b"), &[("hero_b".to_owned(), 1)]);
    state.spawn_army(P1, c, Some("hero_c"), &[("hero_c".to_owned(), 1)]);
    let mut verdict = Verdict::new(false, 1);
    let (recruitable, _) = with_ctx(&state, P1, 3, &mut verdict, |ctx| {
        let barracks = ctx.catalogue.roles.barracks.unwrap();
        let spawner = ctx.catalogue.spawners.get(&barracks).unwrap();
        let picked = recruit::pick(ctx, barracks, spawner, HERO);
        (
            (1..=3)
                .map(|e| recruit::recruitable(ctx, barracks, e))
                .collect::<Vec<_>>(),
            picked.is_some_and(|e| (1..=3).contains(&e)),
        )
    });
    assert_eq!(recruitable.0, [false, false, false]);
    assert!(!recruitable.1, "a held hero was picked");
}
