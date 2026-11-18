use super::*;
use crate::diag::diag;
use crate::match_stats::{record, EndStatus, PlayerFigures, Sample, SAMPLE_PERIOD};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScoreClass {
    Structure,
    Unit,
}

impl RotwkRules {
    pub(super) fn has_kind_of(&self, kind: crate::unit::Kind, name: &str) -> bool {
        self.kinds
            .get(kind)
            .is_some_and(|entry| entry.record.has_kind(name))
    }

    fn score_class(&self, kind: crate::unit::Kind, structure_alt: &str) -> Option<ScoreClass> {
        let record = &self.kinds.get(kind)?.record;
        if record.has_kind("STRUCTURE")
            && (record.has_kind("SCORE") || record.has_kind(structure_alt))
        {
            return Some(ScoreClass::Structure);
        }
        self.objects_that_score
            .passes(&record.kind_of, &record.name)
            .then_some(ScoreClass::Unit)
    }

    fn is_hero_template(&self, template: u16) -> bool {
        let kind = crate::unit::Kind(template);
        self.has_kind_of(kind, "HERO") && !self.has_kind_of(kind, "SUMMONED")
    }

    fn owner_stats_entity(&self, tick: &Tick<'_>, ids: Ids, owner: PlayerId) -> Option<Entity> {
        crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
    }

    pub(super) fn note_produced(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
        scoring: crate::unit::Kind,
        members: u32,
    ) {
        let Some(player) = self.owner_stats_entity(tick, ids, owner) else {
            return;
        };
        let now = u32::try_from(tick.frame().get()).unwrap_or(u32::MAX);
        let hero = self.is_hero_template(kind.0);
        let members = if self.score_class(scoring, "SCORE_CREATE") == Some(ScoreClass::Unit) {
            members
        } else {
            0
        };
        record(tick, ids.match_stats, player, |stats| {
            stats.add_unit_built(kind.0, members);
            if hero && stats.first_hero_tick.is_none() {
                stats.first_hero_tick = Some(now);
            }
        });
    }

    pub(super) fn note_structure_placed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
    ) {
        if self.score_class(kind, "SCORE_CREATE") != Some(ScoreClass::Structure) {
            return;
        }
        if let Some(player) = self.owner_stats_entity(tick, ids, owner) {
            record(tick, ids.match_stats, player, |stats| {
                stats.add_structure_built(kind.0);
            });
        }
    }

    pub(super) fn note_structure_cancelled(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
    ) {
        if self.score_class(kind, "SCORE_CREATE") != Some(ScoreClass::Structure) {
            return;
        }
        if let Some(player) = self.owner_stats_entity(tick, ids, owner) {
            record(tick, ids.match_stats, player, |stats| {
                stats.remove_structure_built(kind.0);
            });
        }
    }

    pub(super) fn note_structure_completed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kind: crate::unit::Kind,
    ) {
        let Some(player) = self.owner_stats_entity(tick, ids, owner) else {
            return;
        };
        let now = u32::try_from(tick.frame().get()).unwrap_or(u32::MAX);
        let fortress = self.has_kind_of(kind, "CASTLE_KEEP");
        if fortress {
            record(tick, ids.match_stats, player, |stats| {
                stats.fortress_ticks.push(now);
            });
        }
    }

    pub(super) fn note_deaths(&self, tick: &mut Tick<'_>, ids: Ids, killed: &[(Entity, f32)]) {
        let mut seen: Vec<Entity> = Vec::with_capacity(killed.len());
        for (victim, _) in killed {
            if seen.contains(victim) {
                continue;
            }
            seen.push(*victim);
            let Some(victim_owner) = tick.state().get(ids.owner, *victim).map(|owner| owner.0)
            else {
                continue;
            };
            let Some(kind) = tick.state().get(ids.kind, *victim).copied() else {
                continue;
            };
            if tick.state().contains(ids.construction, *victim) {
                continue;
            }
            let killer_owner = tick
                .state()
                .get(ids.killing_blow, *victim)
                .and_then(|blow| blow.killer)
                .and_then(|killer| tick.state().get(ids.owner, killer))
                .map(|owner| owner.0);
            let enemy = killer_owner.filter(|killer| self.hostile(*killer, victim_owner));
            if let Some(player) = enemy.and_then(|k| self.owner_stats_entity(tick, ids, k)) {
                record(tick, ids.match_stats, player, |stats| {
                    stats.add_kill_of(kind.0)
                });
            }
            let Some(class) = self.score_class(kind, "SCORE_DESTROY") else {
                continue;
            };
            let structure = class == ScoreClass::Structure;
            if let Some(player) = self.owner_stats_entity(tick, ids, victim_owner) {
                record(tick, ids.match_stats, player, |stats| {
                    if structure {
                        stats.add_structure_lost(kind.0);
                    } else {
                        stats.add_unit_lost(kind.0);
                    }
                });
            }
            if let Some(killer_owner) = enemy {
                if let Some(player) = self.owner_stats_entity(tick, ids, killer_owner) {
                    record(tick, ids.match_stats, player, |stats| {
                        stats.add_kill(killer_owner, victim_owner, structure);
                    });
                }
                if let Some(player) = self.owner_stats_entity(tick, ids, victim_owner) {
                    record(tick, ids.match_stats, player, |stats| {
                        stats.last_enemy = Some(killer_owner);
                    });
                }
            }
        }
    }

    pub(super) fn advance_match_stats(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let frame = tick.frame();
        let now = u32::try_from(frame.get()).unwrap_or(u32::MAX);
        let defeat = Self::defeat_ids(ids);
        let defeated: Vec<PlayerId> =
            crate::defeat::defeat_facts(tick.state(), defeat, frame).to_vec();
        let decided = tick
            .state()
            .store(ids.outcome)
            .is_some_and(|store| store.iter().next().is_some());
        let sampling = now % SAMPLE_PERIOD == 0;
        let counts = if sampling {
            self.live_counts(tick, ids)
        } else {
            Vec::new()
        };
        for slot in 0..self.players {
            let player = PlayerId::new(slot);
            let Some(entity) = self.owner_stats_entity(tick, ids, player) else {
                continue;
            };
            let Some(stats) = tick.state().get(ids.match_stats, entity) else {
                continue;
            };
            if stats.end_tick.is_some() {
                continue;
            }
            if decided || defeated.contains(&player) {
                if defeated.contains(&player) {
                    self.award_elimination(tick, ids, player);
                }
                record(tick, ids.match_stats, entity, |stats| {
                    stats.end_tick = Some(now);
                });
                self.report_stats(tick, ids, player);
                continue;
            }
            if !sampling {
                continue;
            }
            let money =
                crate::economy::purse_of(tick.state(), Self::economy_ids(ids), player).amount();
            let (units, structures) = counts.get(usize::from(slot)).copied().unwrap_or((0, 0));
            let Some(stats) = tick.state().get(ids.match_stats, entity) else {
                continue;
            };
            let sample = Sample {
                money: i32::try_from(money).unwrap_or(i32::MAX),
                score: stats.score(player, &self.score_weights),
                units: i16::try_from(units).unwrap_or(i16::MAX),
                structures: i16::try_from(structures).unwrap_or(i16::MAX),
            };
            let index = usize::try_from(now / SAMPLE_PERIOD).unwrap_or(usize::MAX);
            record(tick, ids.match_stats, entity, |stats| {
                stats.write_sample(index, sample);
            });
        }
        Ok(())
    }

    fn award_elimination(&self, tick: &mut Tick<'_>, ids: Ids, defeated: PlayerId) {
        let Some(entity) = self.owner_stats_entity(tick, ids, defeated) else {
            return;
        };
        let Some(stats) = tick.state().get(ids.match_stats, entity) else {
            return;
        };
        let Some(enemy) = stats.last_enemy else {
            return;
        };
        let bonus = stats.samples().last().map_or(0.0, |sample| sample.score)
            * self.score_weights.player_eliminated;
        if let Some(winner) = self.owner_stats_entity(tick, ids, enemy) {
            record(tick, ids.match_stats, winner, |stats| {
                stats.elimination_bonus += bonus;
            });
        }
    }

    pub(super) fn note_vetted(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        if tick.state().contains(ids.member, entity) {
            return;
        }
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return;
        };
        if self.has_kind_of(kind, "STRUCTURE") {
            return;
        }
        let hero = self.has_kind_of(kind, "HERO");
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return;
        };
        if let Some(player) = self.owner_stats_entity(tick, ids, owner) {
            record(tick, ids.match_stats, player, |stats| {
                if hero {
                    stats.heroes_vetted = stats.heroes_vetted.saturating_add(1);
                } else {
                    stats.units_vetted = stats.units_vetted.saturating_add(1);
                }
            });
        }
    }

    pub(super) fn note_points(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        skill: f32,
        power: u32,
    ) {
        if let Some(entity) = self.owner_stats_entity(tick, ids, player) {
            record(tick, ids.match_stats, entity, |stats| {
                stats.skill_points += skill;
                stats.power_points = stats.power_points.saturating_add(power);
            });
        }
    }

    fn live_counts(&self, tick: &Tick<'_>, ids: Ids) -> Vec<(u32, u32)> {
        let mut counts = vec![(0_u32, 0_u32); usize::from(self.players)];
        let world = tick.state();
        let Some(store) = world.store(ids.kind) else {
            return counts;
        };
        for (entity, kind) in store.iter() {
            if world.contains(ids.doomed, entity) {
                continue;
            }
            let Some(owner) = world.get(ids.owner, entity) else {
                continue;
            };
            let Some(slot) = counts.get_mut(usize::from(owner.0.get())) else {
                continue;
            };
            match self.score_class(*kind, "SCORE_CREATE") {
                Some(ScoreClass::Structure) => {
                    if !world.contains(ids.construction, entity) {
                        slot.1 = slot.1.saturating_add(1);
                    }
                }
                Some(ScoreClass::Unit) => slot.0 = slot.0.saturating_add(1),
                None => {}
            }
        }
        counts
    }

    pub(crate) fn match_figures(
        &self,
        world: &World,
        ids: Ids,
        now: u32,
        quit: Option<PlayerId>,
    ) -> Vec<PlayerFigures> {
        let winner = crate::clock_entity(world, ids)
            .and_then(|clock| world.get(Self::defeat_ids(ids).outcome, clock))
            .map(|outcome| outcome.winner);
        let favourite_kinds = ["HORDE", "HERO", "MONSTER", "MACHINE", "SHIP", "MINE"];
        (0..self.players)
            .filter_map(|slot| {
                let player = PlayerId::new(slot);
                let entity = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
                let stats = world.get(ids.match_stats, entity)?;
                let status = if quit == Some(player) || self.player_defeated(world, ids, player) {
                    EndStatus::Defeated
                } else if winner.is_some_and(|winner| self.allied(winner, player)) {
                    EndStatus::Victorious
                } else {
                    EndStatus::Undecided
                };
                let favourite = stats
                    .built()
                    .iter()
                    .filter(|(template, _)| {
                        let kind = crate::unit::Kind(*template);
                        !self.has_kind_of(kind, "DEPLOYED_MINE")
                            && !self.has_kind_of(kind, "HORDE_MONSTER")
                            && favourite_kinds
                                .iter()
                                .any(|name| self.has_kind_of(kind, name))
                    })
                    .max_by_key(|(template, count)| (*count, std::cmp::Reverse(*template)))
                    .and_then(|(template, _)| self.kinds.get(crate::unit::Kind(*template)))
                    .and_then(|entry| entry.record.display_name.clone());
                Some(PlayerFigures {
                    player,
                    side: self
                        .player_sides()
                        .iter()
                        .find(|(id, _)| *id == player)
                        .map(|(_, side)| side.clone()),
                    status,
                    end_tick: stats.end_tick.unwrap_or(now),
                    structures_built: stats.structures_built,
                    structures_lost: stats.structures_lost,
                    structures_destroyed: stats.structures_destroyed(),
                    fortresses: u32::try_from(stats.fortress_ticks.len()).unwrap_or(u32::MAX),
                    units_built: stats.units_built,
                    units_lost: stats.units_lost,
                    units_killed: stats.units_killed(),
                    favourite,
                    earned: stats.earned,
                    spent: stats.spent(),
                    spent_on_structures: stats.spent_on_structures,
                    spent_on_units: stats.spent_on_units,
                    spent_on_heroes: stats.spent_on_heroes,
                    first_hero_tick: stats.first_hero_tick,
                    heroes_built: stats.built_where(|template| self.is_hero_template(template)),
                    heroes_lost: stats.lost_where(|template| self.is_hero_template(template)),
                    fortress_ticks: stats.fortress_ticks.clone(),
                    samples: stats.samples().to_vec(),
                })
            })
            .collect()
    }

    fn report_stats(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) {
        if let Some(line) = self.stats_line(tick.state(), ids, player, 0) {
            diag!("stats: {line}");
        }
    }

    pub(crate) fn match_stats_lines(&self, world: &World, ids: Ids, now: u32) -> Vec<String> {
        (0..self.players)
            .filter_map(|slot| self.stats_line(world, ids, PlayerId::new(slot), now))
            .map(|line| format!("stats: {line}"))
            .collect()
    }

    fn stats_line(&self, world: &World, ids: Ids, player: PlayerId, now: u32) -> Option<String> {
        let entity = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
        let stats = world.get(ids.match_stats, entity)?;
        let seconds = stats.end_tick.unwrap_or(now) / mint_sim::SIMULATION_HZ;
        let samples = stats.samples().len();
        Some(format!(
            "player {} at {seconds}s - built {} units {} structures, lost {} units {} structures,              killed {} units {} structures, earned {}, spent {}, heroes built {} lost {},              first hero {:?}, fortresses {}, {samples} sample(s), score {:.0}",
            player.get(),
            stats.units_built,
            stats.structures_built,
            stats.units_lost,
            stats.structures_lost,
            stats.units_killed(),
            stats.structures_destroyed(),
            stats.earned,
            stats.spent(),
            stats.built_where(|template| self.is_hero_template(template)),
            stats.lost_where(|template| self.is_hero_template(template)),
            stats.first_hero_tick.map(|tick| tick / mint_sim::SIMULATION_HZ),
            stats.fortress_ticks.len(),
            stats.score(player, &self.score_weights),
        ))
    }
}
