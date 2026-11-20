use super::*;
use mint_sage_ini::game_data::RetreatTuning;

const HUMAN: PlayerId = PlayerId::new(0);
const AI: PlayerId = PlayerId::new(1);
const AI_ALLY: PlayerId = PlayerId::new(2);

const TUNING: RetreatTuning = RetreatTuning {
    farming_threshold: 1000,
    army_quality_bias: 2,
    army_quantity_bias: 1,
    hero_quality_bias: 1,
    map_control_bias: 2,
    base_strength_bias: 1,
    ring_ownership_bias: 2,
    frames_till_checks_start: 10,
    frames_between_checks: 5,
    frames_till_self_destruct: 4,
    disable_tactical_ai: false,
};

fn run(ally_is_human: bool) -> (Option<u64>, Option<u64>, Option<u64>, i64) {
    let (data, kinds) = construction_kinds();
    let spawn = |name: &str, at: f32, player| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(at, 100.0),
                Facing::EAST,
                player,
            )
            .unwrap()
    };
    let objects = vec![
        spawn("Shed", 100.0, HUMAN),
        spawn("Shed", 300.0, AI),
        spawn("Attacker", 340.0, AI),
        spawn("Shed", 500.0, AI_ALLY),
    ];
    let structure =
        crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let seeds = |players: &[PlayerId]| -> Vec<crate::skirmish_ai::AiSeed> {
        players
            .iter()
            .map(|&player| crate::skirmish_ai::AiSeed {
                player,
                candidates: Vec::new(),
            })
            .collect()
    };
    let ais = if ally_is_human {
        vec![AI]
    } else {
        vec![AI, AI_ALLY]
    };
    let mut simulation = Simulation::new(
        64,
        RotwkRules::new(passability, objects)
            .with_combat(data.combat.clone())
            .with_victory(Arc::clone(&kinds), structure)
            .with_players(3)
            .with_teams(vec![Some(0), Some(1), Some(1)])
            .with_skirmish_ai(seeds(&ais), crate::combat::AiTiming::default(), Vec::new())
            .with_ai_power_tuning(
                [(1, 1); 4],
                ais.iter()
                    .map(|&p| {
                        (
                            p,
                            if p == AI {
                                crate::lobby::Difficulty::Medium
                            } else {
                                crate::lobby::Difficulty::Easy
                            },
                        )
                    })
                    .collect(),
            )
            .with_forfeit(Some(TUNING)),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (mut ai_down, mut ally_down, mut warned) = (None, None, None);
    for _ in 0..400 {
        simulation.tick().unwrap();
        let frame = simulation.frame().get();
        let rules = simulation.rules();
        let world = simulation.state();
        if ai_down.is_none() && rules.player_defeated(world, ids, AI) {
            ai_down = Some(frame);
        }
        if ally_down.is_none() && rules.player_defeated(world, ids, AI_ALLY) {
            ally_down = Some(frame);
        }
        if warned.is_none()
            && !rules
                .forfeit_warnings(world, frame.saturating_sub(1))
                .is_empty()
        {
            warned = Some(frame.saturating_sub(1));
        }
    }
    let purse =
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), AI).amount();
    (ai_down, ally_down, warned, purse)
}

#[test]
fn a_failing_ai_warns_then_self_destructs_with_its_purse_emptied() {
    let (ai_down, _, warned, purse) = run(false);
    assert_eq!(warned, Some(25 * 6), "warning, retail frame 25");
    assert_eq!(ai_down, Some(28 * 6 + 1), "self-destruct, retail frame 28");
    assert_eq!(purse, 0);
}

#[test]
fn the_forfeiting_ai_takes_an_allied_ai_with_it_unless_a_human_ally_stands() {
    let (ai_down, ally_down, _, _) = run(false);
    assert_eq!(ally_down, ai_down);
    let (ai_down, ally_down, _, _) = run(true);
    assert!(ai_down.is_some());
    assert_eq!(ally_down, None);
}
