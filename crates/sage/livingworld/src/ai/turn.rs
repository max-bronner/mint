use std::collections::BTreeMap;

use crate::ai::catalogue::Catalogue;
use crate::ai::construct::BuildAction;
use crate::ai::data::AiTemplate;
use crate::ai::retreat::{battle_answer, retreat_choice};
use crate::ai::snapshot::Snapshot;
use crate::ai::BattleAnswer;
use crate::ai::{construct, garrison, hero, recruit, Ctx, MoveKind, Plan, Predictor, Roll, Rolls};
use crate::model::{ArmyId, Command, Phase, PlayerId, PlayerKind, RegionId};
use crate::state::LivingWorldState;

pub const CAMPAIGN_DIFFICULTY: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skill {
    Easy,
    Medium,
    Hard,
    Brutal,
}

impl Skill {
    #[must_use]
    pub fn difficulty(self) -> u8 {
        match self {
            Self::Easy => 2,
            Self::Medium => 3,
            Self::Hard => 4,
            Self::Brutal => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSeat {
    pub player: PlayerId,
    pub template: Option<String>,
    pub difficulty: u8,
}

impl AiSeat {
    #[must_use]
    pub fn campaign(player: PlayerId) -> Self {
        Self {
            player,
            template: None,
            difficulty: CAMPAIGN_DIFFICULTY,
        }
    }

    #[must_use]
    pub fn from_slot(player: PlayerId, skill: Skill) -> Self {
        Self {
            player,
            template: None,
            difficulty: skill.difficulty(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Skipped {
    pub recruits: usize,
    pub unaffordable_builds: usize,
    pub illegal_moves: usize,
    pub retreat_fallbacks: usize,
}

pub(super) struct Seat {
    pub(super) player: PlayerId,
    pub(super) template_name: Option<String>,
    template: AiTemplate,
    catalogue: Catalogue,
    pub(super) difficulty: u8,
    pub(super) turn: u32,
    pub(super) substep: u8,
    pub(super) money: i32,
    pub(super) plan: Plan,
    pub(super) done: bool,
    pub(super) retreated: Option<u32>,
}

pub struct StrategicAi {
    pub(super) seats: Vec<Seat>,
    pub(super) snapshot: Option<(u32, Snapshot)>,
    rolls: Rolls,
    pub skipped: Skipped,
}

type Commands = Vec<(PlayerId, Command)>;

impl StrategicAi {
    #[must_use]
    pub fn new(state: &LivingWorldState, seats: &[AiSeat], record: bool) -> Self {
        let seats = seats
            .iter()
            .filter_map(|s| {
                let p = state.player(s.player)?;
                Some(Seat {
                    player: s.player,
                    template_name: s.template.clone(),
                    template: state.rules.ai.template(s.template.as_deref()),
                    catalogue: Catalogue::build(&state.rules, p.template),
                    difficulty: s.difficulty,
                    turn: 0,
                    substep: 0,
                    money: 0,
                    plan: Plan::default(),
                    done: false,
                    retreated: None,
                })
            })
            .collect();
        Self {
            seats,
            snapshot: None,
            rolls: Rolls::new(state.rng, record),
            skipped: Skipped::default(),
        }
    }

    #[must_use]
    pub fn for_computers(state: &LivingWorldState, difficulty: u8, record: bool) -> Self {
        let seats: Vec<AiSeat> = state
            .players
            .iter()
            .filter(|p| p.kind == PlayerKind::Computer)
            .map(|p| AiSeat {
                player: p.id,
                template: None,
                difficulty,
            })
            .collect();
        Self::new(state, &seats, record)
    }

    #[must_use]
    pub fn for_slots(state: &LivingWorldState, record: bool) -> Self {
        let seats: Vec<AiSeat> = state
            .players
            .iter()
            .filter(|p| p.kind == PlayerKind::Computer)
            .map(|p| AiSeat::from_slot(p.id, p.skill))
            .collect();
        Self::new(state, &seats, record)
    }

    pub fn take_rolls(&mut self) -> Vec<Roll> {
        self.rolls.take_log()
    }

    #[must_use]
    pub fn plan(&self, player: PlayerId) -> Option<&Plan> {
        self.seats
            .iter()
            .find(|s| s.player == player)
            .map(|s| &s.plan)
    }

    pub fn step(
        &mut self,
        state: &mut LivingWorldState,
        predictor: &mut dyn Predictor,
    ) -> Option<Commands> {
        self.rolls.load(state.rng);
        let out = match state.phase {
            Phase::Planning => self.planning(state, predictor),
            Phase::PlanRetreats => self.retreats(state, predictor),
            _ => None,
        };
        state.rng = *self.rolls.rng();
        out
    }

    pub fn battle_answer(
        &mut self,
        state: &mut LivingWorldState,
        predictor: &mut dyn Predictor,
        player: PlayerId,
        region: RegionId,
    ) -> BattleAnswer {
        let Some(seat) = self.seats.iter().find(|s| s.player == player) else {
            return BattleAnswer::Abstain;
        };
        self.rolls.load(state.rng);
        let snap = Snapshot::take(state);
        let mut ctx = Ctx {
            state,
            snap: &snap,
            catalogue: &seat.catalogue,
            template: &seat.template,
            me: player,
            difficulty: seat.difficulty,
            rolls: &mut self.rolls,
            predictor,
        };
        let answer = battle_answer(&mut ctx, region);
        state.rng = *self.rolls.rng();
        answer
    }

    fn active(state: &LivingWorldState, seat: &Seat) -> bool {
        state
            .player(seat.player)
            .is_some_and(|p| !p.defeated && p.kind == PlayerKind::Computer && !p.ready)
    }

    fn planning(
        &mut self,
        state: &mut LivingWorldState,
        predictor: &mut dyn Predictor,
    ) -> Option<Commands> {
        for seat in &mut self.seats {
            if seat.turn != state.turn {
                seat.turn = state.turn;
                seat.substep = 0;
                seat.done = false;
                seat.plan = Plan::default();
            }
        }
        if self
            .snapshot
            .as_ref()
            .is_some_and(|(t, _)| *t != state.turn)
        {
            self.snapshot = None;
        }
        let Some(index) = self
            .seats
            .iter()
            .position(|s| Self::active(state, s) && !s.done)
        else {
            return self.advance_hook(state);
        };
        if self.snapshot.is_none() {
            self.snapshot = Some((state.turn, Snapshot::take(state)));
        }
        let (_, snap) = self.snapshot.as_ref()?;
        let seat = self.seats.get_mut(index)?;
        let mut ctx = Ctx {
            state,
            snap,
            catalogue: &seat.catalogue,
            template: &seat.template,
            me: seat.player,
            difficulty: seat.difficulty,
            rolls: &mut self.rolls,
            predictor,
        };
        let me = seat.player;
        let mut commands = Commands::new();
        match seat.substep {
            0 => {
                seat.money = snap.summary(me).map_or(0, |s| s.money);
                seat.plan = Plan::default();
            }
            1 => {
                let budget = seat.money.saturating_mul(65) / 100;
                let (orders, left) = recruit::plan(&mut ctx, budget);
                seat.money = seat.money.saturating_sub(budget.saturating_sub(left));
                seat.plan.recruits = orders;
            }
            2 => seat.plan.builds = construct::plan(&mut ctx, seat.money),
            3 => garrison::plan(&mut ctx, &mut seat.plan.moves),
            _ => {
                hero::plan(&mut ctx, &mut seat.plan.moves);
                commands = execute(state, seat, &mut self.skipped);
                seat.done = true;
            }
        }
        seat.substep = seat.substep.saturating_add(1);
        Some(commands)
    }

    fn advance_hook(&mut self, state: &LivingWorldState) -> Option<Commands> {
        let waiting: Vec<usize> = (0..self.seats.len())
            .filter(|i| {
                self.seats
                    .get(*i)
                    .is_some_and(|s| Self::active(state, s) && s.done)
            })
            .collect();
        if waiting.is_empty() {
            return None;
        }
        let mut commands = Commands::new();
        let mut extra: BTreeMap<ArmyId, i32> = BTreeMap::new();
        for i in waiting {
            let seat = self.seats.get(i)?;
            heroless(state, seat, &mut extra, &mut commands, &mut self.skipped);
            commands.push((seat.player, Command::EndPhase));
        }
        Some(commands)
    }

    fn retreats(
        &mut self,
        state: &LivingWorldState,
        predictor: &mut dyn Predictor,
    ) -> Option<Commands> {
        let index = self.seats.iter().position(|s| {
            s.retreated != Some(state.turn)
                && state
                    .player(s.player)
                    .is_some_and(|p| !p.defeated && p.kind == PlayerKind::Computer)
        })?;
        self.snapshot = None;
        let seat = self.seats.get_mut(index)?;
        seat.retreated = Some(state.turn);
        seat.substep = 0;
        let pending = state.retreat_pending(seat.player);
        let mut commands = Commands::new();
        if pending.is_empty() {
            return Some(commands);
        }
        let snap = Snapshot::take(state);
        let mut ctx = Ctx {
            state,
            snap: &snap,
            catalogue: &seat.catalogue,
            template: &seat.template,
            me: seat.player,
            difficulty: seat.difficulty,
            rolls: &mut self.rolls,
            predictor,
        };
        for army in pending {
            let Some(chosen) = retreat_choice(&mut ctx, army) else {
                continue;
            };
            let legal = |dest: RegionId| state.check_order(seat.player, army, dest, true).is_ok();
            let dest = if legal(chosen) {
                Some(chosen)
            } else {
                self.skipped.retreat_fallbacks = self.skipped.retreat_fallbacks.saturating_add(1);
                state
                    .armies
                    .get(&army)
                    .and_then(|a| state.rules.region_rules(a.region))
                    .and_then(|r| r.neighbours.iter().copied().find(|n| legal(*n)))
            };
            if let Some(dest) = dest {
                commands.push((seat.player, Command::ChooseRetreat { army, dest }));
            }
        }
        Some(commands)
    }
}

pub(super) fn execute(state: &LivingWorldState, seat: &Seat, skipped: &mut Skipped) -> Commands {
    let me = seat.player;
    let mut commands = Commands::new();
    let mut balance = state.player(me).map_or(0, |p| p.resources);
    let ceiling = state
        .player(me)
        .and_then(|p| state.rules.templates.get(usize::from(p.template.0)))
        .map_or(i32::MAX, |t| t.max_resources);
    let mut queued: Vec<(RegionId, u8)> = Vec::new();
    let mut queued_heroes: Vec<crate::model::UnitId> = Vec::new();
    for order in &seat.plan.recruits {
        let slot = state
            .region(order.region)
            .and_then(|r| r.plots.get(usize::from(order.plot))?.as_ref());
        let queue = slot
            .and_then(|p| state.rules.building(p.building))
            .and_then(|b| b.queue.as_ref());
        let (Some(slot), Some(queue), Ok(entry)) = (slot, queue, u8::try_from(order.entry)) else {
            skipped.recruits = skipped.recruits.saturating_add(1);
            continue;
        };
        let entry_army = queue.entries.get(order.entry);
        let first = entry_army
            .and_then(|e| e.roster.first())
            .map(|(unit, _)| *unit);
        let hero = entry_army.and_then(|e| state.queued_hero(e));
        if hero.is_some_and(|u| !state.hero_recruitable(me, u) || queued_heroes.contains(&u)) {
            skipped.recruits = skipped.recruits.saturating_add(1);
            continue;
        }
        let price = first.map_or(0, |unit| state.unit_price(me, unit));
        let waiting = queued
            .iter()
            .filter(|q| **q == (order.region, order.plot))
            .count()
            .saturating_add(slot.queue.len());
        if balance < price || i64::try_from(waiting).unwrap_or(i64::MAX) >= i64::from(queue.size) {
            skipped.recruits = skipped.recruits.saturating_add(1);
            continue;
        }
        balance = balance.saturating_sub(price).min(ceiling);
        queued.push((order.region, order.plot));
        if let Some(unit) = hero {
            queued_heroes.push(unit);
        }
        commands.push((
            me,
            Command::QueueArmy {
                region: order.region,
                plot: order.plot,
                entry,
            },
        ));
    }
    let mut freed: Vec<(RegionId, u8)> = Vec::new();
    for order in &seat.plan.builds {
        match order.action {
            BuildAction::Demolish { plot } => {
                freed.push((order.region, plot));
                commands.push((
                    me,
                    Command::Demolish {
                        region: order.region,
                        plot,
                    },
                ));
            }
            BuildAction::Build(building) => {
                let plot = freed
                    .iter()
                    .find(|(r, _)| *r == order.region)
                    .map(|(_, p)| *p)
                    .or_else(|| {
                        let slot = state
                            .region(order.region)?
                            .plots
                            .iter()
                            .position(Option::is_none)?;
                        u8::try_from(slot).ok()
                    });
                let cost = state.rules.building(building).map_or(0, |b| b.cost);
                match plot {
                    Some(plot) if balance >= cost => {
                        balance = balance.saturating_sub(cost).min(ceiling);
                        commands.push((
                            me,
                            Command::Build {
                                region: order.region,
                                plot,
                                building,
                            },
                        ));
                    }
                    Some(_) => {
                        skipped.unaffordable_builds = skipped.unaffordable_builds.saturating_add(1)
                    }
                    None => {}
                }
            }
        }
    }
    for order in seat.plan.moves.iter().filter(|m| !m.heroless) {
        let MoveKind::Move { region } = order.kind else {
            continue;
        };
        let Some(army) = state.armies.get(&order.army) else {
            continue;
        };
        if army.region == region || army.player != me {
            continue;
        }
        if state.check_order(me, order.army, region, false).is_ok() {
            commands.push((
                me,
                Command::MoveArmy {
                    army: order.army,
                    dest: region,
                },
            ));
        } else {
            skipped.illegal_moves = skipped.illegal_moves.saturating_add(1);
        }
    }
    commands
}

fn heroless(
    state: &LivingWorldState,
    seat: &Seat,
    extra: &mut BTreeMap<ArmyId, i32>,
    commands: &mut Commands,
    skipped: &mut Skipped,
) {
    let me = seat.player;
    for order in seat.plan.moves.iter().filter(|m| m.heroless) {
        let Some(mover) = state.armies.get(&order.army) else {
            continue;
        };
        match order.kind {
            MoveKind::Move { region } => {
                let takes = state.armies_of(me).any(|d| {
                    d.id != mover.id
                        && !d.is_hero_army()
                        && d.region == region
                        && state.army_cp(d).saturating_add(state.army_cp(mover))
                            <= state.army_cap(d)
                });
                if !takes {
                    continue;
                }
                if state.check_order(me, mover.id, region, false).is_ok() {
                    commands.push((
                        me,
                        Command::MoveArmy {
                            army: mover.id,
                            dest: region,
                        },
                    ));
                } else {
                    skipped.illegal_moves = skipped.illegal_moves.saturating_add(1);
                }
            }
            MoveKind::Transfer { to, member } => {
                let (Some(target), Some(found)) = (
                    state.armies.get(&to),
                    mover.roster.iter().find(|m| m.id == member),
                ) else {
                    continue;
                };
                if Some(found.unit) == mover.hero || state.rules.unit(found.unit).hero {
                    continue;
                }
                let each = state.rules.unit(found.unit).command_points;
                let cp = each.saturating_mul(i32::try_from(found.count).unwrap_or(i32::MAX));
                let added = extra.entry(to).or_insert(0);
                let held = state
                    .army_cp(target)
                    .saturating_add(*added)
                    .saturating_add(cp);
                let overfull = if cp > 0 {
                    held > state.army_cap(target)
                } else {
                    state.army_cp(mover) > state.army_cap(mover)
                };
                if overfull
                    || state
                        .check_order(me, mover.id, target.region, false)
                        .is_err()
                {
                    skipped.illegal_moves = skipped.illegal_moves.saturating_add(1);
                    continue;
                }
                *added = added.saturating_add(cp);
                commands.push((
                    me,
                    Command::SwapArmyMembers {
                        a: mover.id,
                        b: to,
                        a_members: vec![member],
                        b_members: Vec::new(),
                    },
                ));
            }
        }
    }
}
