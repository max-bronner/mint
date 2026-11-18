use mint_math::rng::Pcg32;

use crate::chain::Chain;
use crate::roster::{
    Army, ArmyResult, Arrival, FallenHero, Kill, Member, Options, Outcome, Participant,
    ParticipantResult, ParticipantRound, RoundLog, SkillTracker, Survivor,
};
use crate::stl;
use crate::tables::{
    floor, Armor, Body, Leadership, LeadershipBonus, Tables, Template, UnitType, Weapon,
};

pub const ROUND_CAP: u32 = 1000;
const REGENERATION_ROUNDS: u32 = 500;

struct Unit<'a> {
    member: Member,
    template: &'a Template,
    weapon: &'a Weapon,
    armor: &'a Armor,
    body: &'a Body,
    chain: &'a Chain,
    leadership: Option<&'a Leadership>,
    hp: f32,
    hp_start: f32,
    max_hp: f32,
    fired: bool,
    bonus: Option<LeadershipBonus>,
    target: Option<usize>,
    army: Option<usize>,
    participant: usize,
}

impl Unit<'_> {
    fn can_fire(&self) -> bool {
        !self.fired && self.hp_start > 0.0 && self.weapon.can_attack()
    }

    fn can_be_attacked(&self) -> bool {
        self.hp > 0.0 && self.body.can_be_attacked
    }

    fn hitpoints(&self) -> f32 {
        self.body.hitpoints_at(self.member.level)
    }

    fn unit_type(&self) -> UnitType {
        self.template.unit_type
    }
}

#[derive(Clone, Copy)]
struct Queued {
    unit: usize,
    target: UnitType,
    priority: i32,
}

struct TargetList {
    entries: Vec<(usize, usize)>,
    head: usize,
}

pub(crate) struct Battle<'a> {
    tables: &'a Tables,
    participants: &'a [Participant],
    options: Options,
    rng: &'a mut Pcg32,
    units: Vec<Unit<'a>>,
    sides: [Vec<usize>; 2],
    arrivals: Vec<Vec<i32>>,
    skills: Vec<Option<SkillTracker>>,
    science: Vec<(f32, f32)>,
    round: u32,
    winner: Option<usize>,
    kills: Vec<Kill>,
    destroyed: Vec<u32>,
    fallen: Vec<FallenHero>,
    log: Vec<RoundLog>,
    opening: RoundLog,
    fields: Vec<Vec<ParticipantRound>>,
    arrivals_due: Vec<Vec<Arrival>>,
    draws: u32,
}

#[must_use]
pub fn resolve(
    participants: &[Participant],
    tables: &Tables,
    options: Options,
    rng: &mut Pcg32,
) -> Outcome {
    let mut battle = Battle::new(participants, tables, options, rng);
    while battle.winner.is_none() && options.round_cap.is_none_or(|cap| battle.round < cap) {
        battle.step_round();
    }
    battle.finish()
}

fn by_level_desc<'u>(units: &'u [Unit<'_>]) -> impl Fn(&usize, &usize) -> bool + 'u {
    move |a, b| {
        let level = |i: &usize| units.get(*i).map_or(0, |u| u.member.level);
        level(a) > level(b)
    }
}

fn queue_order(a: &Queued, b: &Queued) -> bool {
    a.priority > b.priority
}

impl<'a> Battle<'a> {
    fn new(
        participants: &'a [Participant],
        tables: &'a Tables,
        options: Options,
        rng: &'a mut Pcg32,
    ) -> Self {
        let arrivals = participants
            .iter()
            .map(|p| {
                let mut k = 0u32;
                p.armies
                    .iter()
                    .map(|army| {
                        if army.named {
                            let schedule = tables.schedules.get(p.side.min(1));
                            let round = schedule.map_or(0, |s| s.round_of(k));
                            k = k.saturating_add(1);
                            round.max(0)
                        } else {
                            0
                        }
                    })
                    .collect()
            })
            .collect();
        let mut battle = Self {
            tables,
            participants,
            options,
            rng,
            units: Vec::new(),
            sides: [Vec::new(), Vec::new()],
            arrivals,
            skills: participants.iter().map(|p| p.skill.clone()).collect(),
            science: vec![(1.0, 1.0); participants.len()],
            round: 0,
            winner: None,
            kills: Vec::new(),
            destroyed: Vec::new(),
            fallen: Vec::new(),
            log: Vec::new(),
            opening: RoundLog {
                round: 0,
                units: [0; 2],
                hit_points: [0.0; 2],
            },
            fields: vec![Vec::new(); participants.len()],
            arrivals_due: Vec::new(),
            draws: 0,
        };
        battle.arrivals_due = battle.arrival_list();
        battle.enter_scheduled(0, true);
        battle.refresh_science();
        battle.opening = battle.snapshot(0);
        battle.record_fields();
        battle.winner = battle.end_check();
        battle
    }

    fn make_unit(
        &self,
        member: &Member,
        participant: usize,
        army: Option<usize>,
    ) -> Option<Unit<'a>> {
        let tables: &'a Tables = self.tables;
        let template = tables.template(&member.template)?;
        let gated = |list: &[crate::tables::Gated]| {
            list.iter()
                .find(|g| g.passes(&member.upgrades))
                .map(|g| g.name.to_ascii_lowercase())
        };
        let weapon = gated(&template.weapons)
            .and_then(|n| tables.weapons.get(&n))
            .unwrap_or(&tables.default_weapon);
        let armor = gated(&template.armors)
            .and_then(|n| tables.armors.get(&n))
            .unwrap_or(&tables.default_armor);
        let body = template
            .body
            .as_ref()
            .and_then(|n| tables.bodies.get(&n.to_ascii_lowercase()))
            .unwrap_or(&tables.default_body);
        let chain = template
            .chain
            .as_ref()
            .and_then(|n| tables.chains.get(&n.to_ascii_lowercase()))
            .unwrap_or(&tables.default_chain);
        let leadership = template
            .leadership
            .as_ref()
            .and_then(|n| tables.leaderships.get(&n.to_ascii_lowercase()));
        let hp = body.hitpoints_at(member.level);
        Some(Unit {
            member: member.clone(),
            template,
            weapon,
            armor,
            body,
            chain,
            leadership,
            hp,
            hp_start: hp,
            max_hp: hp,
            fired: false,
            bonus: None,
            target: None,
            army,
            participant,
        })
    }

    fn enter_scheduled(&mut self, round: u32, with_initial: bool) {
        let participants = self.participants;
        let mut entering: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
        for (p, participant) in participants.iter().enumerate() {
            let side = participant.side.min(1);
            let mut rosters: Vec<(&[Member], Option<usize>)> = Vec::new();
            if with_initial {
                rosters.push((&participant.initial_units, None));
            }
            for (a, army) in participant.armies.iter().enumerate() {
                let arrival = self
                    .arrivals
                    .get(p)
                    .and_then(|r| r.get(a))
                    .copied()
                    .unwrap_or(0);
                if u32::try_from(arrival).ok() == Some(round) {
                    rosters.push((&army.members, Some(a)));
                }
            }
            for (members, army) in rosters {
                for member in members {
                    if let Some(unit) = self.make_unit(member, p, army) {
                        if let Some(batch) = entering.get_mut(side) {
                            batch.push(self.units.len());
                        }
                        self.units.push(unit);
                    }
                }
                if let Some(batch) = entering.get_mut(side) {
                    stl::sort(batch, &by_level_desc(&self.units));
                }
            }
        }
        for (side, added) in entering.into_iter().enumerate() {
            if added.is_empty() {
                continue;
            }
            if let Some(list) = self.sides.get_mut(side) {
                let existing = list.len();
                list.extend(added);
                stl::merge_stable(list, existing, &by_level_desc(&self.units));
            }
        }
    }

    fn refresh_science(&mut self) {
        for (p, participant) in self.participants.iter().enumerate() {
            let points = self
                .skills
                .get(p)
                .and_then(|s| s.as_ref())
                .map_or(0, |s| s.purchase_points);
            let step = participant
                .science_bonus
                .iter()
                .filter(|s| s.min_points <= points)
                .max_by_key(|s| s.min_points);
            if let Some(slot) = self.science.get_mut(p) {
                *slot = step.map_or((1.0, 1.0), |s| (s.weapon, s.armor));
            }
        }
    }

    fn end_check(&self) -> Option<usize> {
        let alive = |s: usize| {
            self.sides.get(s).is_some_and(|l| {
                l.iter()
                    .any(|&u| self.units.get(u).is_some_and(Unit::can_be_attacked))
            })
        };
        if !alive(1) {
            Some(0)
        } else if !alive(0) {
            Some(1)
        } else {
            None
        }
    }

    fn step_round(&mut self) {
        self.round = self.round.saturating_add(1);
        self.enter_scheduled(self.round, false);
        for unit in &mut self.units {
            unit.hp_start = unit.hp;
            unit.max_hp = unit.hitpoints();
            unit.fired = false;
        }
        for side in 0..2 {
            self.assign_leadership(side);
        }
        loop {
            self.assign_targets();
            if self.fire() {
                break;
            }
        }
        let capped = self.end_of_round();
        for side in 0..2 {
            if let Some(list) = self.sides.get_mut(side) {
                stl::sort(list, &by_level_desc(&self.units));
            }
        }
        self.winner = capped.or_else(|| self.end_check());
        let entry = self.snapshot(self.round);
        self.log.push(entry);
        self.record_fields();
    }

    fn arrival_list(&self) -> Vec<Vec<Arrival>> {
        self.participants
            .iter()
            .enumerate()
            .map(|(p, participant)| {
                let mut list: Vec<Arrival> = Vec::new();
                for (a, army) in participant.armies.iter().enumerate() {
                    let arrival = self
                        .arrivals
                        .get(p)
                        .and_then(|r| r.get(a))
                        .copied()
                        .and_then(|r| u32::try_from(r).ok())
                        .unwrap_or(0);
                    let hit_points: f32 = army
                        .members
                        .iter()
                        .filter_map(|m| self.make_unit(m, p, Some(a)))
                        .map(|u| u.hp.max(0.0))
                        .sum();
                    list.push(Arrival {
                        army: army.id,
                        round: arrival,
                        hit_points,
                    });
                }
                list.sort_by_key(|b| b.round);
                list
            })
            .collect()
    }

    fn record_fields(&mut self) {
        for (p, track) in self.fields.iter_mut().enumerate() {
            let mut units = 0usize;
            let mut hit_points = 0.0f32;
            for side in &self.sides {
                for unit in side.iter().filter_map(|&u| self.units.get(u)) {
                    if unit.participant == p {
                        units = units.saturating_add(1);
                        hit_points += unit.hp.max(0.0);
                    }
                }
            }
            track.push(ParticipantRound { units, hit_points });
        }
    }

    fn snapshot(&self, round: u32) -> RoundLog {
        let hit_points = |s: usize| {
            self.sides.get(s).map_or(0.0, |l| {
                l.iter()
                    .filter_map(|&u| self.units.get(u))
                    .map(|u| u.hp.max(0.0))
                    .sum()
            })
        };
        RoundLog {
            round,
            units: [
                self.sides.first().map_or(0, Vec::len),
                self.sides.get(1).map_or(0, Vec::len),
            ],
            hit_points: [hit_points(0), hit_points(1)],
        }
    }

    fn assign_leadership(&mut self, side: usize) {
        let list = self.sides.get(side).cloned().unwrap_or_default();
        let mut offers: Vec<(i32, usize)> = Vec::new();
        for &u in &list {
            let Some(unit) = self.units.get(u) else {
                continue;
            };
            if let Some(bonus) = unit
                .leadership
                .and_then(|l| floor(&l.bonuses, unit.member.level))
            {
                offers.push((bonus.priority, u));
            }
        }
        stl::sort(&mut offers, &|a: &(i32, usize), b: &(i32, usize)| a.0 > b.0);
        for (_, leader) in offers {
            let Some((def, bonus)) = self.units.get(leader).and_then(|l| {
                let def = l.leadership?;
                Some((def, floor(&def.bonuses, l.member.level)?))
            }) else {
                continue;
            };
            let order: Vec<usize> = if def.affects_higher_level_first {
                list.clone()
            } else {
                list.iter().rev().copied().collect()
            };
            let mut given = 0;
            for u in order {
                if given >= bonus.maximum_units_affected {
                    break;
                }
                if u == leader {
                    continue;
                }
                if let Some(unit) = self.units.get_mut(u) {
                    if unit.bonus.is_none()
                        && def.affects.contains(&unit.unit_type())
                        && unit.hp > 0.0
                    {
                        unit.bonus = Some(bonus);
                        given = given.saturating_add(1);
                    }
                }
            }
        }
    }

    fn assign_targets(&mut self) {
        for unit in &mut self.units {
            unit.target = None;
        }
        let mut attackers = [0usize; 2];
        let mut targets = [0usize; 2];
        let mut queues: [Vec<Queued>; 2] = [Vec::new(), Vec::new()];
        let mut lists: Vec<Vec<TargetList>> = (0..2)
            .map(|_| {
                (0..UnitType::ALL.len())
                    .map(|_| TargetList {
                        entries: Vec::new(),
                        head: 0,
                    })
                    .collect()
            })
            .collect();
        for side in 0..2 {
            for &u in self.sides.get(side).map_or(&[][..], Vec::as_slice) {
                let Some(unit) = self.units.get(u) else {
                    continue;
                };
                if unit.can_fire() {
                    if let Some(n) = attackers.get_mut(side) {
                        *n = n.saturating_add(1);
                    }
                    if let (Some((target, priority)), Some(queue)) =
                        (unit.chain.first(), queues.get_mut(side))
                    {
                        queue.push(Queued {
                            unit: u,
                            target,
                            priority,
                        });
                        stl::push_heap(queue, &queue_order);
                    }
                }
                if unit.can_be_attacked() {
                    if let Some(n) = targets.get_mut(side) {
                        *n = n.saturating_add(1);
                    }
                    if let Some(list) = lists
                        .get_mut(side)
                        .and_then(|l| l.get_mut(unit.unit_type().index()))
                    {
                        list.entries.push((u, 0));
                    }
                }
            }
        }
        for side in 0..2 {
            let other = usize::from(side == 0);
            let total = targets.get(other).copied().unwrap_or(0);
            if total == 0 {
                continue;
            }
            let attacking = attackers.get(side).copied().unwrap_or(0);
            let (Some(quota), Some(mut remainder)) =
                (attacking.checked_div(total), attacking.checked_rem(total))
            else {
                continue;
            };
            while let Some(queue) = queues.get_mut(side) {
                if queue.is_empty() {
                    break;
                }
                stl::pop_heap(queue, &queue_order);
                let Some(item) = queue.pop() else {
                    break;
                };
                let mut assigned = None;
                if let Some(list) = lists
                    .get_mut(other)
                    .and_then(|l| l.get_mut(item.target.index()))
                {
                    while let Some(entry) = list.entries.get_mut(list.head) {
                        if entry.1 < quota || (entry.1 == quota && remainder > 0) {
                            if entry.1 == quota {
                                remainder = remainder.saturating_sub(1);
                            }
                            entry.1 = entry.1.saturating_add(1);
                            assigned = Some(entry.0);
                            break;
                        }
                        list.head = list.head.saturating_add(1);
                    }
                }
                let Some(unit) = self.units.get_mut(item.unit) else {
                    continue;
                };
                if assigned.is_some() {
                    unit.target = assigned;
                } else if let Some((target, priority)) = unit.chain.next(item.target, item.priority)
                {
                    queue.push(Queued {
                        unit: item.unit,
                        target,
                        priority,
                    });
                    stl::push_heap(queue, &queue_order);
                }
            }
        }
    }

    fn fire(&mut self) -> bool {
        let mut done = true;
        for side in 0..2 {
            let order = self.sides.get(side).cloned().unwrap_or_default();
            for u in order {
                let Some(attacker) = self.units.get(u) else {
                    continue;
                };
                if !attacker.can_fire() {
                    continue;
                }
                let Some(t) = attacker.target else {
                    continue;
                };
                let Some(victim) = self.units.get(t) else {
                    continue;
                };
                if victim.hp <= 0.0 {
                    done = false;
                    continue;
                }
                if !self.options.tutorial {
                    let roll = self.rng.below(100).saturating_add(1);
                    self.draws = self.draws.saturating_add(1);
                    if i64::from(roll) <= i64::from(attacker.weapon.miss_percent) {
                        if let Some(a) = self.units.get_mut(u) {
                            a.fired = true;
                        }
                        continue;
                    }
                }
                let damage = self.shot_damage(u, t);
                let killed = self.units.get_mut(t).is_some_and(|v| {
                    v.hp -= damage;
                    v.hp <= 0.0
                });
                if let Some(a) = self.units.get_mut(u) {
                    a.fired = true;
                }
                if killed {
                    self.credit_kill(u, t);
                }
            }
        }
        done
    }

    fn shot_damage(&self, attacker: usize, victim: usize) -> f32 {
        let (Some(u), Some(t)) = (self.units.get(attacker), self.units.get(victim)) else {
            return 0.0;
        };
        let level = u.member.level;
        let d = u
            .weapon
            .damage_against(level, u.hp_start, u.hitpoints(), t.unit_type());
        let part = |i: usize| self.participants.get(i);
        let pw = part(u.participant).map_or(1.0, |p| p.weapon_multiplier);
        let pa = part(t.participant).map_or(1.0, |p| p.armor_multiplier);
        let (sw, _) = self
            .science
            .get(u.participant)
            .copied()
            .unwrap_or((1.0, 1.0));
        let (_, sa) = self
            .science
            .get(t.participant)
            .copied()
            .unwrap_or((1.0, 1.0));
        let (la, lw) = (
            t.bonus.map_or(1.0, |b| b.armor),
            u.bonus.map_or(1.0, |b| b.weapon),
        );
        let armor = t
            .armor
            .taken
            .get(u.unit_type().index())
            .copied()
            .unwrap_or(1.0);
        let mut x = pa * pw;
        x = x * sa * sw;
        x = x * la * armor * lw * d;
        x
    }

    #[allow(clippy::cast_possible_truncation)]
    fn award(&self, victim: usize) -> i32 {
        let Some(t) = self.units.get(victim) else {
            return 0;
        };
        let xp = t.member.experience.trunc() as i32;
        let quantity = t.member.quantity.max(0);
        if !t.template.horde {
            return quantity.saturating_mul(t.template.award_at(xp));
        }
        let mut total = 0i32;
        for (name, count) in &t.template.initial_payload {
            if let Some(payload) = self.tables.template(name) {
                let each = (*count).max(0).saturating_mul(quantity);
                total = total.saturating_add(each.saturating_mul(payload.award_at(xp)));
            }
        }
        total
    }

    fn credit_kill(&mut self, attacker: usize, victim: usize) {
        let award = self.award(victim);
        let Some(u) = self.units.get(attacker) else {
            return;
        };
        let participant = u.participant;
        let px = self
            .participants
            .get(participant)
            .map_or(1.0, |p| p.experience_multiplier);
        let mut v = px * award as f32;
        if let Some(skill) = self.skills.get_mut(participant).and_then(|s| s.as_mut()) {
            skill.add(v, false);
        }
        if let Some(bonus) = u.bonus {
            v *= bonus.experience;
        }
        let victim_participant = self.units.get(victim).map_or(0, |t| t.participant);
        if let Some(u) = self.units.get_mut(attacker) {
            add_experience(u, v.trunc());
            u.member.kills = u.member.kills.saturating_add(1);
        }
        self.kills.push(Kill {
            round: self.round,
            attacker: participant,
            victim: victim_participant,
        });
    }

    fn remove_unit(&mut self, u: usize) {
        let Some(unit) = self.units.get(u) else {
            return;
        };
        if let Some(handle) = unit.member.object {
            self.destroyed.push(handle);
        }
        if let (true, Some(army)) = (unit.template.hero, unit.army) {
            if let Some(id) = self
                .participants
                .get(unit.participant)
                .and_then(|p| p.armies.get(army))
                .map(|a: &Army| a.id)
            {
                self.fallen.push(FallenHero {
                    participant: unit.participant,
                    army: id,
                    member: unit.member.clone(),
                });
            }
        }
    }

    fn sweep(&mut self, side: usize, upkeep: bool) {
        let list = self.sides.get(side).cloned().unwrap_or_default();
        let mut kept = Vec::with_capacity(list.len());
        for &u in list.iter().rev() {
            let Some(unit) = self.units.get(u) else {
                continue;
            };
            if unit.hp <= 0.0 && !unit.body.leave_in_army_summary {
                self.remove_unit(u);
                continue;
            }
            let Some(unit) = self.units.get_mut(u) else {
                continue;
            };
            if upkeep {
                let max = unit.hitpoints();
                if max != unit.max_hp && unit.max_hp != 0.0 {
                    unit.hp *= max / unit.max_hp;
                }
                if self.round < REGENERATION_ROUNDS && unit.hp > 0.0 {
                    unit.hp = (unit.hp + unit.body.regeneration_at(unit.member.level)).min(max);
                }
                unit.bonus = None;
            }
            kept.push(u);
        }
        kept.reverse();
        if let Some(slot) = self.sides.get_mut(side) {
            *slot = kept;
        }
    }

    fn end_of_round(&mut self) -> Option<usize> {
        for side in 0..2 {
            self.sweep(side, true);
        }
        self.refresh_science();
        if self.round >= ROUND_CAP {
            let count = |s: usize| self.sides.get(s).map_or(0, Vec::len);
            let winner = usize::from(count(1) > count(0));
            let loser = usize::from(winner == 0);
            for &u in self.sides.get(loser).map_or(&[][..], Vec::as_slice) {
                if let Some(unit) = self.units.get_mut(u) {
                    unit.hp = 0.0;
                }
            }
            self.sweep(loser, false);
            return Some(winner);
        }
        None
    }

    fn finish(self) -> Outcome {
        let winner = self.winner.unwrap_or(0);
        let mut participants: Vec<ParticipantResult> = self
            .participants
            .iter()
            .enumerate()
            .map(|(p, participant)| ParticipantResult {
                armies: participant
                    .armies
                    .iter()
                    .enumerate()
                    .map(|(a, army)| ArmyResult {
                        id: army.id,
                        touched: self
                            .arrivals
                            .get(p)
                            .and_then(|r| r.get(a))
                            .is_some_and(|&r| u32::try_from(r).is_ok_and(|r| r <= self.round)),
                        survivors: Vec::new(),
                    })
                    .collect(),
                skill_points: participant.skill.as_ref().map(|s| s.points),
                purchase_points_delta: participant.skill.as_ref().map(|s| {
                    s.purchase_points
                        .saturating_sub(participant.legendary_bonus)
                }),
                lost: participant.side != winner,
            })
            .collect();
        for side in &self.sides {
            for &u in side {
                let Some(unit) = self.units.get(u) else {
                    continue;
                };
                let Some(army) = unit.army else {
                    continue;
                };
                if let Some(result) = participants
                    .get_mut(unit.participant)
                    .and_then(|p| p.armies.get_mut(army))
                {
                    result.survivors.push(Survivor {
                        member: unit.member.clone(),
                    });
                }
            }
        }
        Outcome {
            winner,
            rounds: self.round,
            participants,
            destroyed_objects: self.destroyed,
            fallen_heroes: self.fallen,
            kills: self.kills,
            log: self.log,
            opening: self.opening,
            fields: self.fields,
            arrivals: self.arrivals_due,
            draws: self.draws,
        }
    }
}

fn add_experience(unit: &mut Unit<'_>, amount: f32) {
    if !unit.template.trainable {
        return;
    }
    let experience = unit.member.experience + amount;
    let reached = experience.trunc();
    let mut levels = unit.template.levels.clone();
    levels.sort_by(|a, b| a.required.total_cmp(&b.required));
    while let Some(next) = levels.iter().find(|l| l.rank > unit.member.level) {
        if reached < next.required.trunc() {
            break;
        }
        unit.member.level = next.rank;
    }
    unit.member.experience = experience;
}

#[cfg(test)]
mod tests;
