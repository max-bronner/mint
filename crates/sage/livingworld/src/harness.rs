#![allow(missing_docs)]

use crate::ai::{BattleAnswer, Predictor, StrategicAi};
use crate::battle::AutoResolve;
use crate::engine::apply_with;
use crate::model::{
    ArmyId, BattleMode, BuildingId, Command, Event, Phase, PlayerId, PlayerKind, RegionId, Rejected,
};
use crate::state::LivingWorldState;

pub type Logged = (PlayerId, Command);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundary {
    pub turn: u32,
    pub phase: Phase,
    pub hash: u64,
}

#[derive(Default)]
struct Sink {
    boundaries: Vec<Boundary>,
    gained: Vec<(PlayerId, u16)>,
}

fn run(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    log: &mut Vec<Logged>,
    boundaries: &mut Sink,
    player: PlayerId,
    command: Command,
) -> Option<Rejected> {
    match apply_with(state, Some(resolver), player, command.clone()) {
        Ok(events) => {
            log.push((player, command));
            for event in events {
                match event {
                    Event::PhaseChanged { turn, phase, hash } => {
                        boundaries.boundaries.push(Boundary { turn, phase, hash });
                    }
                    Event::TerritoryGained { player, territory } => {
                        boundaries.gained.push((player, territory));
                    }
                    _ => {}
                }
            }
            None
        }
        Err(reason) => Some(reason),
    }
}

fn first_order(
    state: &LivingWorldState,
    army: ArmyId,
    pick: usize,
    retreat: bool,
) -> Option<RegionId> {
    let a = state.armies.get(&army)?;
    let neighbours = &state.rules.region_rules(a.region)?.neighbours;
    let n = neighbours.len();
    (0..n)
        .filter_map(|i| {
            neighbours
                .get(i.saturating_add(pick).checked_rem(n)?)
                .copied()
        })
        .find(|dest| state.check_order(a.player, army, *dest, retreat).is_ok())
}

fn plan(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    log: &mut Vec<Logged>,
    boundaries: &mut Sink,
    driven: bool,
) {
    let players: Vec<PlayerId> = state
        .live_players()
        .filter(|p| p.kind != PlayerKind::Inert)
        .filter(|p| !(driven && p.kind == PlayerKind::Computer))
        .map(|p| p.id)
        .collect();
    let turn = usize::try_from(state.turn).unwrap_or(0);
    for player in players {
        let armies: Vec<ArmyId> = state.armies_of(player).map(|a| a.id).collect();
        for (index, army) in armies.into_iter().enumerate() {
            let pick = turn
                .saturating_add(index)
                .saturating_add(usize::from(player.0));
            if let Some(dest) = first_order(state, army, pick, false) {
                run(
                    state,
                    resolver,
                    log,
                    boundaries,
                    player,
                    Command::MoveArmy { army, dest },
                );
            }
        }
        let build = (0..state.regions.len()).find_map(|r| {
            let region = RegionId(u16::try_from(r).ok()?);
            let slot = state
                .region(region)?
                .plots
                .iter()
                .position(Option::is_none)?;
            let building = (0..state.rules.buildings.len()).find_map(|b| {
                let id = BuildingId(u16::try_from(b).ok()?);
                (state.owner(region) == Some(player)).then_some(id)
            })?;
            Some((region, u8::try_from(slot).ok()?, building))
        });
        if let Some((region, plot, building)) = build {
            run(
                state,
                resolver,
                log,
                boundaries,
                player,
                Command::Build {
                    region,
                    plot,
                    building,
                },
            );
        }
        run(state, resolver, log, boundaries, player, Command::EndPhase);
    }
}

const REFUSED_KEPT: usize = 16;

pub struct AiRun<'a> {
    pub ai: &'a mut StrategicAi,
    pub predictor: &'a mut dyn Predictor,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub boundaries: Vec<Boundary>,
    pub gained: Vec<(PlayerId, u16)>,
    pub rejected: usize,
    pub refused: Vec<(PlayerId, Command, Rejected, Phase)>,
}

pub fn play(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    turns: u32,
    log: &mut Vec<Logged>,
) -> Vec<Boundary> {
    drive(state, resolver, None, turns, log).boundaries
}

pub fn play_with_ai(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    ai: AiRun<'_>,
    turns: u32,
    log: &mut Vec<Logged>,
) -> Outcome {
    drive(state, resolver, Some(ai), turns, log)
}

fn drive(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    mut ai: Option<AiRun<'_>>,
    turns: u32,
    log: &mut Vec<Logged>,
) -> Outcome {
    let mut boundaries = Sink::default();
    let mut rejected = 0usize;
    let mut refused = Vec::new();
    let target = state.turn.saturating_add(turns);
    let mut idle = 0u32;
    while state.turn < target && state.ended.is_none() && idle < 4 {
        let before = (state.turn, state.phase, log.len());
        let mut stepped = false;
        if matches!(state.phase, Phase::Planning | Phase::PlanRetreats) {
            if let Some(run_ai) = ai.as_mut() {
                if let Some(commands) = run_ai.ai.step(state, &mut *run_ai.predictor) {
                    stepped = true;
                    for (player, command) in commands {
                        let sent = command.clone();
                        if let Some(reason) =
                            run(state, resolver, log, &mut boundaries, player, command)
                        {
                            rejected = rejected.saturating_add(1);
                            if refused.len() < REFUSED_KEPT {
                                refused.push((player, sent, reason, state.phase));
                            }
                        }
                    }
                }
            }
        }
        let driven = ai.is_some();
        match state.phase {
            _ if stepped => {}
            Phase::Planning => plan(state, resolver, log, &mut boundaries, driven),
            Phase::ResolveBattles => {
                if let (Some(run_ai), Some(battle)) = (ai.as_mut(), state.battles.first().cloned())
                {
                    for p in battle.players.iter().copied() {
                        let asked = state.kind(p) == Some(PlayerKind::Computer)
                            && !battle.choices.iter().any(|(c, _)| *c == p);
                        if !asked || !state.battles.iter().any(|b| b.id == battle.id) {
                            continue;
                        }
                        let method = match run_ai.ai.battle_answer(
                            state,
                            &mut *run_ai.predictor,
                            p,
                            battle.region,
                        ) {
                            BattleAnswer::AutoResolve => 1,
                            BattleAnswer::Withdraw => 2,
                            BattleAnswer::RealTime => 3,
                            BattleAnswer::Undecided | BattleAnswer::Abstain => continue,
                        };
                        let command = Command::ChooseBattleMode {
                            battle: battle.id,
                            method,
                        };
                        run(state, resolver, log, &mut boundaries, p, command);
                    }
                }
                let work = state
                    .battles
                    .first()
                    .and_then(|b| Some((b.region, *b.players.first()?)));
                if let Some((region, player)) = work {
                    let command = Command::ResolveBattle {
                        region,
                        mode: BattleMode::Auto,
                    };
                    run(state, resolver, log, &mut boundaries, player, command);
                }
                let choice = state
                    .decisions
                    .values()
                    .find(|d| !d.resolved)
                    .and_then(|d| {
                        Some((state.decision_chooser(d)?, d.region, *d.players.first()?))
                    });
                if let Some((chooser, region, player)) = choice {
                    let command = Command::ChooseSharedCapture { region, player };
                    run(state, resolver, log, &mut boundaries, chooser, command);
                }
            }
            Phase::PlanRetreats => {
                let pending: Vec<(PlayerId, ArmyId)> = state
                    .players
                    .iter()
                    .filter(|p| !(driven && p.kind == PlayerKind::Computer))
                    .flat_map(|p| state.retreat_pending(p.id).into_iter().map(|a| (p.id, a)))
                    .collect();
                for (player, army) in pending {
                    if let Some(dest) = first_order(state, army, 0, true) {
                        let command = Command::ChooseRetreat { army, dest };
                        run(state, resolver, log, &mut boundaries, player, command);
                    }
                }
            }
            _ => {}
        }
        idle = if !stepped && before == (state.turn, state.phase, log.len()) {
            idle.saturating_add(1)
        } else {
            0
        };
    }
    Outcome {
        boundaries: boundaries.boundaries,
        gained: boundaries.gained,
        rejected,
        refused,
    }
}

pub fn replay(
    state: &mut LivingWorldState,
    resolver: &mut dyn AutoResolve,
    log: &[Logged],
) -> Vec<Boundary> {
    let mut boundaries = Vec::new();
    for (player, command) in log {
        if let Ok(events) = apply_with(state, Some(resolver), *player, command.clone()) {
            for event in events {
                if let Event::PhaseChanged { turn, phase, hash } = event {
                    boundaries.push(Boundary { turn, phase, hash });
                }
            }
        }
    }
    boundaries
}
