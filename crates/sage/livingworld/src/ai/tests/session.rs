use super::{rid, session, Verdict, P0, P1};
use crate::ai::{AiSeat, Roll, Skill, StrategicAi};
use crate::engine::apply;
use crate::harness::{play_with_ai, AiRun};
use crate::model::{Command, PlayerId, PlayerKind};
use crate::state::LivingWorldState;
use crate::tests::AttackerLoses;

type Commands = Vec<(PlayerId, Command)>;

fn frames(
    state: &mut LivingWorldState,
    ai: &mut StrategicAi,
    frames: usize,
    strict: bool,
) -> Vec<Commands> {
    let mut verdict = Verdict::new(false, 3);
    let mut out = Vec::new();
    for _ in 0..frames {
        let commands = ai
            .step(state, &mut verdict)
            .expect("a computer player has work");
        for (player, command) in &commands {
            let result = apply(state, *player, command.clone());
            assert!(result.is_ok() || !strict, "{command:?} refused: {result:?}");
        }
        out.push(commands);
    }
    out
}

#[allow(clippy::cast_possible_truncation)]
fn shape(rolls: &[Roll]) -> Vec<(char, i32, i32)> {
    rolls
        .iter()
        .map(|r| match *r {
            Roll::Int { lo, hi, .. } => ('i', lo, hi),
            Roll::Real { lo, hi, drew, .. } => (if drew { 'r' } else { 'n' }, lo as i32, hi as i32),
        })
        .collect()
}

fn lone_computer(extra: &str) -> LivingWorldState {
    let mut state = session(
        &[(PlayerKind::Human, "A"), (PlayerKind::Computer, "E")],
        8,
        extra,
        3,
    );
    state.armies.clear();
    state
}

#[test]
fn a_turn_draws_in_the_decoded_order() {
    let mut state = lone_computer(" BuildingScoreFarm = 10\n BuildingScoreCastle = 7\n");
    for name in ["F", "G"] {
        let id = rid(&state, name);
        state.regions[id.index()].owner = Some(P1);
    }
    let mut ai = StrategicAi::new(&state, &[AiSeat::from_slot(P1, Skill::Medium)], true);
    frames(&mut state, &mut ai, 5, true);
    let farm_then_castle = [('r', 0, 10), ('r', 0, 7)];
    let expected: Vec<(char, i32, i32)> = std::iter::repeat_n(farm_then_castle, 3)
        .flatten()
        .chain([('i', 0, 1)])
        .collect();
    assert_eq!(shape(&ai.take_rolls()), expected);
}

#[test]
fn keep_scores_draw_again_for_a_full_region() {
    let mut state = lone_computer(
        " BuildingScoreFarm = 1\n BuildingScoreBarracks = 1\n BuildingScoreArmory = 1\n BuildingScoreCastle = 0\n",
    );
    state.players[1].resources = 1000;
    let e = rid(&state, "E");
    for name in ["Farm", "Barracks", "Armory"] {
        let building = state.rules.building_id(name).unwrap();
        let plot = u8::try_from(
            state.regions[e.index()]
                .plots
                .iter()
                .position(Option::is_none)
                .unwrap(),
        )
        .unwrap();
        apply(
            &mut state,
            P1,
            Command::Build {
                region: e,
                plot,
                building,
            },
        )
        .unwrap();
        let built = state.regions[e.index()].plots[usize::from(plot)]
            .as_mut()
            .unwrap();
        built.under_construction = false;
        built.turns_left = 0;
    }
    let d = rid(&state, "D");
    state.spawn_army(P0, d, Some("hero_a"), &[("hero_a".to_owned(), 1)]);
    let mut ai = StrategicAi::new(&state, &[AiSeat::from_slot(P1, Skill::Medium)], true);
    frames(&mut state, &mut ai, 5, true);
    let expected = vec![
        ('i', 0, 0),
        ('r', 0, 1),
        ('n', 0, 0),
        ('r', 0, 1),
        ('r', 0, 1),
        ('i', 0, 1),
    ];
    assert_eq!(shape(&ai.take_rolls()), expected);
}

#[test]
fn the_second_ai_plans_on_the_snapshot_taken_before_the_first_acted() {
    let mut state = session(
        &[(PlayerKind::Computer, "A"), (PlayerKind::Computer, "H")],
        8,
        " BuildingScoreFarm = 10\n",
        3,
    );
    state.armies.clear();
    let g = rid(&state, "G");
    state.regions[g.index()].owner = Some(P1);
    let mut ai = StrategicAi::for_computers(&state, 3, false);
    frames(&mut state, &mut ai, 5, true);
    state.regions[g.index()].owner = Some(P0);
    frames(&mut state, &mut ai, 5, false);
    let builds = &ai.plan(P1).unwrap().builds;
    assert!(builds.iter().any(|b| b.region == g), "{builds:?}");
}

#[test]
fn a_garrison_moves_at_the_advance_not_with_the_heroes() {
    for seed in 1..64 {
        let mut state = session(
            &[(PlayerKind::Human, "A"), (PlayerKind::Computer, "E")],
            8,
            "",
            seed,
        );
        state.armies.clear();
        let (d, e) = (rid(&state, "D"), rid(&state, "E"));
        state.regions[d.index()].owner = Some(P1);
        let mover = state.spawn_army(P1, e, None, &[("elf".to_owned(), 1)]);
        state.spawn_army(P1, d, None, &[("elf".to_owned(), 1)]);
        let mut ai = StrategicAi::new(&state, &[AiSeat::from_slot(P1, Skill::Medium)], false);
        let run = frames(&mut state, &mut ai, 5, true);
        let planned = ai
            .plan(P1)
            .unwrap()
            .moves
            .iter()
            .any(|m| m.army == mover && m.heroless);
        if !planned {
            continue;
        }
        let moves_mover = |c: &Commands| {
            c.iter()
                .any(|(_, c)| matches!(c, Command::MoveArmy { army, .. } if *army == mover))
        };
        assert!(
            !run.iter().any(moves_mover),
            "heroless move sent with the heroes"
        );
        let advance = ai.step(&mut state, &mut Verdict::new(false, 3)).unwrap();
        assert!(moves_mover(&advance));
        assert!(matches!(advance.last(), Some((_, Command::EndPhase))));
        return;
    }
    panic!("no seed gave the garrison a move");
}

#[test]
fn four_computer_players_are_deterministic_and_never_rejected() {
    let seats = [
        (PlayerKind::Computer, "A"),
        (PlayerKind::Computer, "C"),
        (PlayerKind::Computer, "F"),
        (PlayerKind::Computer, "H"),
    ];
    let run = || {
        let mut state = session(&seats, 8, " BuildingScoreFarm = 10\n BuildingScoreCastle = 7\n BuildingScoreBarracks = 5\n BuildingScoreArmory = 4\n", 11);
        let mut ai = StrategicAi::for_computers(&state, 4, false);
        let mut verdict = Verdict::new(false, 3);
        let mut log = Vec::new();
        let outcome = play_with_ai(
            &mut state,
            &mut AttackerLoses,
            AiRun {
                ai: &mut ai,
                predictor: &mut verdict,
            },
            25,
            &mut log,
        );
        (outcome, state.turn, state.state_hash())
    };
    let (first, turn, hash) = run();
    assert_eq!(first.rejected, 0);
    assert!(turn > 10, "turn {turn}");
    assert!(!first.boundaries.is_empty());
    assert_eq!(run(), (first, turn, hash));
}
