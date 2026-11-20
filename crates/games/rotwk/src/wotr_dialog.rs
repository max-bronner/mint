use std::collections::VecDeque;

use mint_sage_autoresolve::roster::Arrival;
use mint_sage_livingworld::PlayerId;

use crate::wotr_resolve::ResolvedBattle;

pub(crate) const WIN: &str = "_win";
pub(crate) const LOSE: &str = "_lose";
pub(crate) const SURVIVED: &str = "_survived";
pub(crate) const ELIMINATED: &str = "_eliminated";

const BANNERS: usize = 3;
pub(crate) const DEFAULT_ROUNDS_PER_STEP: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Panel {
    Allies,
    Enemies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) panel: Panel,
    pub(crate) index: u32,
}

impl Slot {
    pub(crate) fn path(self) -> String {
        match self.panel {
            Panel::Allies => format!("main.allies.{}", self.index),
            Panel::Enemies => format!("main.enemies.{}", self.index),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Call {
    NumPlayers(Panel, u32),
    Identity(usize),
    Health(Slot, f32),
    Hit(Slot, f32),
    Reinforce(Slot, f32),
    UnitCount(Slot, usize),
    Banner(Slot, u32, bool),
    State(Slot, &'static str),
    Result(&'static str),
    FadeOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Post {
    Step { target: u32 },
    Skip,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    Call(Call),
    Post(Post),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    HitDone,
    Chain,
    ReinforceDone,
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) slot: Slot,
    pub(crate) player: PlayerId,
    hit_points: Vec<f32>,
    units: Vec<usize>,
    arrivals: Vec<Arrival>,
    heroes: Vec<Option<String>>,
}

impl Entry {
    pub(crate) fn banner_hero(&self, index: u32) -> Option<&str> {
        let at = usize::try_from(index).ok()?;
        self.heroes.get(at)?.as_deref()
    }

    fn hp_at(&self, round: u32) -> f32 {
        let at = usize::try_from(round).unwrap_or(usize::MAX);
        self.hit_points
            .get(at)
            .or(self.hit_points.last())
            .copied()
            .unwrap_or(0.0)
    }

    fn units_at(&self, round: u32) -> usize {
        let at = usize::try_from(round).unwrap_or(usize::MAX);
        self.units
            .get(at)
            .or(self.units.last())
            .copied()
            .unwrap_or(0)
    }

    fn entered(&self, after: u32, upto: u32) -> usize {
        self.arrivals
            .iter()
            .filter(|a| a.round > after && a.round <= upto)
            .count()
    }

    fn due_after(&self, round: u32) -> bool {
        self.arrivals.iter().any(|a| a.round > round)
    }

    fn total(&self) -> f32 {
        self.hp_at(0)
            + self
                .arrivals
                .iter()
                .filter(|a| a.round > 0)
                .map(|a| a.hit_points)
                .sum::<f32>()
    }

    pub(crate) fn banners_at_open(&self) -> usize {
        self.arrivals
            .iter()
            .filter(|a| a.round == 0)
            .count()
            .min(BANNERS)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(crate) entries: Vec<Entry>,
    pub(crate) whole: f32,
    pub(crate) rounds: u32,
    pub(crate) victory: bool,
}

impl Plan {
    pub(crate) fn count(&self, panel: Panel) -> u32 {
        let n = self
            .entries
            .iter()
            .filter(|e| e.slot.panel == panel)
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    fn percent(&self, hp: f32) -> f32 {
        if self.whole <= 0.0 {
            return 0.0;
        }
        (hp.max(0.0) / self.whole * 100.0).clamp(0.0, 100.0)
    }
}

pub(crate) fn plan(battle: &ResolvedBattle, local: PlayerId) -> Option<Plan> {
    let viewer = battle
        .sides
        .iter()
        .position(|players| players.contains(&local))?;
    let outcome = &battle.outcome;
    let mut allies = Vec::new();
    let mut enemies = Vec::new();
    for (participant, player) in battle.order.iter().enumerate() {
        let ally = battle.side_of(participant) == viewer;
        let list = if ally { &mut allies } else { &mut enemies };
        let panel = if ally { Panel::Allies } else { Panel::Enemies };
        let field = outcome
            .fields
            .get(participant)
            .map_or(&[][..], Vec::as_slice);
        let arrivals = outcome
            .arrivals
            .get(participant)
            .map_or(&[][..], Vec::as_slice);
        list.push(Entry {
            slot: Slot {
                panel,
                index: u32::try_from(list.len()).unwrap_or(u32::MAX),
            },
            player: *player,
            hit_points: field.iter().map(|f| f.hit_points).collect(),
            units: field.iter().map(|f| f.units).collect(),
            arrivals: arrivals.to_vec(),
            heroes: arrivals
                .iter()
                .map(|a| battle.heroes.get(&a.army).cloned())
                .collect(),
        });
    }
    allies.extend(enemies);
    let whole = allies.iter().map(Entry::total).fold(0.0f32, f32::max);
    Some(Plan {
        entries: allies,
        whole,
        rounds: outcome.rounds,
        victory: outcome.winner == viewer,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Loading,
    Reinforcing,
    Hitting,
    Finished,
}

pub(crate) struct Playback {
    plan: Plan,
    step: u32,
    stage: Stage,
    target: u32,
    reached: u32,
    old: Vec<f32>,
    shown: Vec<f32>,
    sent_health: Vec<f32>,
    sent_state: Vec<Option<&'static str>>,
    banners: Vec<usize>,
    loaded: Vec<bool>,
    waiting: usize,
    queue: VecDeque<usize>,
    flying: VecDeque<usize>,
    hits_left: usize,
}

fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0e-3
}

impl Playback {
    pub(crate) fn start(plan: Plan, step: u32) -> (Self, Vec<Out>) {
        let n = plan.entries.len();
        let playback = Self {
            step: step.max(1),
            stage: Stage::Loading,
            loaded: vec![false; n],
            target: step.max(1),
            reached: 0,
            old: plan.entries.iter().map(|e| e.hp_at(0)).collect(),
            shown: plan
                .entries
                .iter()
                .map(|e| plan.percent(e.hp_at(0)))
                .collect(),
            sent_health: vec![100.0; n],
            sent_state: vec![None; n],
            banners: vec![0; n],
            waiting: 0,
            queue: VecDeque::new(),
            flying: VecDeque::new(),
            hits_left: 0,
            plan,
        };
        let out = vec![
            Out::Call(Call::NumPlayers(
                Panel::Allies,
                playback.plan.count(Panel::Allies),
            )),
            Out::Call(Call::NumPlayers(
                Panel::Enemies,
                playback.plan.count(Panel::Enemies),
            )),
            Out::Post(Post::Step {
                target: playback.target,
            }),
        ];
        (playback, out)
    }

    pub(crate) fn panel_loaded(&mut self, panel: Panel, index: u32) -> Vec<Out> {
        let Some(i) = self
            .plan
            .entries
            .iter()
            .position(|e| e.slot.panel == panel && e.slot.index == index)
        else {
            return Vec::new();
        };
        let Some((slot, units)) = self.plan.entries.get(i).map(|e| (e.slot, e.units_at(0))) else {
            return Vec::new();
        };
        let again = self.loaded.get(i).copied().unwrap_or(false);
        let mut out = vec![Out::Call(Call::Identity(i))];
        let shown = self.shown.get(i).copied().unwrap_or(100.0);
        if again {
            if let Some(sent) = self.sent_health.get_mut(i) {
                *sent = 100.0;
            }
        }
        self.set_health(i, shown, &mut out);
        out.push(Out::Call(Call::UnitCount(slot, units)));
        let banners = if again {
            self.banners.get(i).copied().unwrap_or(0)
        } else {
            self.plan.entries.get(i).map_or(0, Entry::banners_at_open)
        };
        for b in 0..banners {
            out.push(Out::Call(Call::Banner(
                slot,
                u32::try_from(b).unwrap_or(0),
                true,
            )));
        }
        if let Some(count) = self.banners.get_mut(i) {
            *count = banners;
        }
        if let Some(done) = self.loaded.get_mut(i) {
            *done = true;
        }
        if self.stage == Stage::Loading && self.loaded.iter().all(|done| *done) {
            self.read_step(&mut out);
        }
        out
    }

    pub(crate) fn finished(&self) -> bool {
        self.stage == Stage::Finished
    }

    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&Entry> {
        self.plan.entries.get(index)
    }

    fn set_health(&mut self, i: usize, pct: f32, out: &mut Vec<Out>) {
        let Some(slot) = self.plan.entries.get(i).map(|e| e.slot) else {
            return;
        };
        if let Some(cache) = self.sent_health.get_mut(i) {
            if !same(*cache, pct) {
                *cache = pct;
                out.push(Out::Call(Call::Health(slot, pct)));
            }
        }
        if let Some(shown) = self.shown.get_mut(i) {
            *shown = pct;
        }
    }

    fn set_state(&mut self, i: usize, state: &'static str, out: &mut Vec<Out>) {
        let Some(slot) = self.plan.entries.get(i).map(|e| e.slot) else {
            return;
        };
        if let Some(cache) = self.sent_state.get_mut(i) {
            if *cache != Some(state) {
                *cache = Some(state);
                out.push(Out::Call(Call::State(slot, state)));
            }
        }
    }

    fn read_step(&mut self, out: &mut Vec<Out>) {
        let reached = self.target.min(self.plan.rounds);
        let over = reached >= self.plan.rounds;
        if !over {
            self.target = reached.saturating_add(self.step);
            out.push(Out::Post(Post::Step {
                target: self.target,
            }));
        }
        let before = self.reached;
        let mut reinforce = Vec::new();
        let mut hits = Vec::new();
        for i in 0..self.plan.entries.len() {
            let Some((slot, now, units, entered)) = self.plan.entries.get(i).map(|e| {
                (
                    e.slot,
                    e.hp_at(reached),
                    e.units_at(reached),
                    e.entered(before, reached),
                )
            }) else {
                continue;
            };
            let was = self.old.get(i).copied().unwrap_or(0.0);
            out.push(Out::Call(Call::UnitCount(slot, units)));
            for _ in 0..entered {
                let next = self.banners.get(i).copied().unwrap_or(BANNERS);
                if next < BANNERS {
                    out.push(Out::Call(Call::Banner(
                        slot,
                        u32::try_from(next).unwrap_or(0),
                        true,
                    )));
                    if let Some(count) = self.banners.get_mut(i) {
                        *count = next.saturating_add(1);
                    }
                }
            }
            if now > was && entered == 0 {
                let pct = self.plan.percent(now);
                self.set_health(i, pct, out);
            }
            if entered > 0 {
                reinforce.push((i, (now - was).max(0.0)));
            }
            if now < was {
                hits.push((i, was - now));
            }
            if let Some(old) = self.old.get_mut(i) {
                *old = now;
            }
        }
        self.reached = reached;
        for (i, gain) in &reinforce {
            let room = 100.0 - self.shown.get(*i).copied().unwrap_or(100.0);
            let amount = self.plan.percent(*gain).min(room.max(0.0));
            if let (Some(slot), Some(shown)) = (
                self.plan.entries.get(*i).map(|e| e.slot),
                self.shown.get_mut(*i),
            ) {
                *shown += amount;
                out.push(Out::Call(Call::Reinforce(slot, amount)));
            }
        }
        self.waiting = reinforce.len();
        hits.sort_by_key(|(i, _)| {
            let enemy = self
                .plan
                .entries
                .get(*i)
                .is_some_and(|e| e.slot.panel == Panel::Enemies);
            (!enemy, *i)
        });
        self.queue = hits.into_iter().map(|(i, _)| i).collect();
        self.hits_left = self.queue.len();
        self.flying.clear();
        if self.waiting > 0 {
            self.stage = Stage::Reinforcing;
        } else {
            self.start_hits(out);
        }
    }

    fn start_hits(&mut self, out: &mut Vec<Out>) {
        if self.hits_left == 0 {
            self.step_done(out);
            return;
        }
        self.stage = Stage::Hitting;
        self.issue_hit(out);
    }

    fn issue_hit(&mut self, out: &mut Vec<Out>) {
        let Some(i) = self.queue.pop_front() else {
            return;
        };
        let Some(slot) = self.plan.entries.get(i).map(|e| e.slot) else {
            return;
        };
        let target = self
            .plan
            .entries
            .get(i)
            .map_or(0.0, |e| e.hp_at(self.reached));
        let target = self.plan.percent(target);
        let shown = self.shown.get(i).copied().unwrap_or(0.0);
        let amount = (shown - target).clamp(0.0, shown.max(0.0));
        if let Some(cell) = self.shown.get_mut(i) {
            *cell -= amount;
        }
        self.flying.push_back(i);
        out.push(Out::Call(Call::Hit(slot, amount)));
    }

    fn step_done(&mut self, out: &mut Vec<Out>) {
        if self.reached >= self.plan.rounds {
            self.finish(out);
        } else {
            self.read_step(out);
        }
    }

    pub(crate) fn event(&mut self, event: Event) -> Vec<Out> {
        let mut out = Vec::new();
        match (self.stage, event) {
            (Stage::Reinforcing, Event::ReinforceDone) => {
                self.waiting = self.waiting.saturating_sub(1);
                if self.waiting == 0 {
                    self.start_hits(&mut out);
                }
            }
            (Stage::Hitting, Event::Chain) => self.issue_hit(&mut out),
            (Stage::Hitting, Event::HitDone) => {
                if let Some(i) = self.flying.pop_front() {
                    let at = self.reached;
                    let gone = self
                        .plan
                        .entries
                        .get(i)
                        .is_some_and(|e| e.hp_at(at) <= 0.0 && !e.due_after(at));
                    if gone {
                        self.set_state(i, ELIMINATED, &mut out);
                    }
                }
                self.hits_left = self.hits_left.saturating_sub(1);
                if self.hits_left == 0 {
                    self.step_done(&mut out);
                } else if self.flying.is_empty() {
                    self.issue_hit(&mut out);
                }
            }
            _ => {}
        }
        out
    }

    pub(crate) fn skip(&mut self) -> Vec<Out> {
        if self.stage == Stage::Finished {
            return Vec::new();
        }
        let mut out = vec![Out::Post(Post::Skip)];
        self.reached = self.plan.rounds;
        self.waiting = 0;
        self.queue.clear();
        self.flying.clear();
        for entry in &self.plan.entries {
            out.push(Out::Call(Call::UnitCount(
                entry.slot,
                entry.units_at(self.plan.rounds),
            )));
        }
        self.finish(&mut out);
        out
    }

    fn finish(&mut self, out: &mut Vec<Out>) {
        self.stage = Stage::Finished;
        let at = self.plan.rounds;
        for i in 0..self.plan.entries.len() {
            let Some((hp, slot)) = self.plan.entries.get(i).map(|e| (e.hp_at(at), e.slot)) else {
                continue;
            };
            let pct = self.plan.percent(hp);
            if let Some(cache) = self.sent_health.get_mut(i) {
                if !same(*cache, pct) {
                    *cache = pct;
                    out.push(Out::Call(Call::Health(slot, pct)));
                }
            }
            let state = if hp > 0.0 { SURVIVED } else { ELIMINATED };
            self.set_state(i, state, out);
        }
        out.push(Out::Call(Call::Result(if self.plan.victory {
            WIN
        } else {
            LOSE
        })));
        out.push(Out::Call(Call::FadeOut));
    }

    pub(crate) fn percent_of(&self, i: usize) -> f32 {
        self.plan
            .entries
            .get(i)
            .map_or(0.0, |e| self.plan.percent(e.hp_at(self.reached)))
    }
}

#[cfg(test)]
mod tests;
