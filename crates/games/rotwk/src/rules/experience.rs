use mint_sage_combat::{
    compute_bonus, Attributes, BonusConditions, ModifierId, WeaponBonus, WeaponTemplate,
};
use mint_sim::{Entity, Frame, PlayerId, Result, Tick, UpgradeMask, World};

use crate::unit::Kind;

use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::experience::{
    scaled, scores_kill, Bonuses, Experience, LevelReadout, LevelTable, RankDisplay,
    WeaponBonusConditions,
};

impl RotwkRules {
    pub(super) fn level_table_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&LevelTable> {
        if let Some(table) = self.hero_table(world, ids, entity) {
            return Some(table);
        }
        let kind = world.get(ids.kind, entity)?;
        self.level_tables.get(*kind)
    }

    pub(crate) fn experience_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<(usize, u32)> {
        let table = self.level_table_of(world, ids, entity)?;
        Some(match world.get(ids.experience, entity) {
            Some(held) => (usize::from(held.level).min(table.top()), held.points),
            None => (0, table.level(0).map_or(0, |level| level.required)),
        })
    }

    pub(super) fn xp_scalar(&self, world: &World, ids: Ids, receiver: Entity) -> f32 {
        let is_structure = world
            .get(ids.kind, receiver)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
        let multiplay = if is_structure {
            self.multiplay.building_xp(self.players)
        } else {
            self.multiplay.unit_xp(self.players)
        };
        world
            .get(ids.bonuses, receiver)
            .map_or(1.0, |bonuses| bonuses.0.experience)
            * multiplay
    }

    pub(super) fn base_rank_of(&self, world: &World, ids: Ids, entity: Entity) -> u16 {
        match world.get(ids.experience, entity) {
            Some(held) => held.base_rank,
            None => self
                .level_table_of(world, ids, entity)
                .and_then(|table| table.level(0))
                .map_or(0, |level| level.rank),
        }
    }

    pub(super) fn level_scalar(&self, world: &World, ids: Ids, receiver: Entity) -> f32 {
        let is_structure = world
            .get(ids.kind, receiver)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
        if is_structure {
            return 1.0;
        }
        let Some(table) = self.level_table_of(world, ids, receiver) else {
            return 1.0;
        };
        let Some((old, _)) = self.experience_of(world, ids, receiver) else {
            return 1.0;
        };
        let Some(level) = table.level(old) else {
            return 1.0;
        };
        let base_rank = self.base_rank_of(world, ids, receiver);
        table.experience_scalar(level.rank, base_rank)
    }

    pub(crate) fn level_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        index: usize,
    ) -> Option<&crate::experience::Level> {
        self.level_table_of(world, ids, entity)?.level(index)
    }

    pub(crate) fn rank_of(&self, world: &World, entity: Entity) -> Option<u16> {
        let ids = self.ids?;
        let (index, _) = self.experience_of(world, ids, entity)?;
        Some(self.level_table_of(world, ids, entity)?.level(index)?.rank)
    }

    pub(crate) fn level_readout(&self, world: &World, entity: Entity) -> Option<LevelReadout> {
        let ids = self.ids?;
        let holder = self.sink_of(world, ids, entity);
        let (index, points) = self.experience_of(world, ids, holder)?;
        self.level_table_of(world, ids, holder)?
            .readout(index, points)
    }

    pub(crate) fn rank_display(
        &self,
        world: &World,
        selected: &[Entity],
        now: Frame,
    ) -> Option<RankDisplay> {
        let mut shown = selected
            .iter()
            .map(|&entity| self.rank_display_of(world, entity, now));
        let first = shown.next()??;
        shown
            .all(|other| other.is_some_and(|other| first.agrees_with(other)))
            .then_some(first)
    }

    fn rank_display_of(&self, world: &World, entity: Entity, now: Frame) -> Option<RankDisplay> {
        let ids = self.ids?;
        let subject = self.command_subject(world, entity);
        let span = world
            .get(ids.lifetime, subject)
            .and_then(crate::summons::Lifetime::span)
            .or_else(|| {
                world
                    .get(ids.defect_revert, subject)
                    .map(|defect| (defect.since, defect.until))
            })
            .or_else(|| self.cloak_timer(world, ids, subject, now));
        match span {
            Some(span) => {
                crate::summons::remaining_share(span, now).map(RankDisplay::TimeRemaining)
            }
            None => self.level_readout(world, entity).map(RankDisplay::Level),
        }
    }

    pub(crate) fn rank_decal(
        &self,
        world: &World,
        entity: Entity,
    ) -> Option<(Entity, &mint_sage_modules::SelectionDecal)> {
        let ids = self.ids?;
        let holder = self.sink_of(world, ids, entity);
        let (index, _) = self.experience_of(world, ids, holder)?;
        let decal = self
            .level_table_of(world, ids, holder)?
            .level(index)?
            .decal
            .as_ref()?;
        Some((holder, decal))
    }

    pub(super) fn level_upgrades(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&UpgradeMask> {
        let (index, _) = self.experience_of(world, ids, entity)?;
        Some(
            &self
                .level_table_of(world, ids, entity)?
                .level(index)?
                .granted,
        )
    }

    pub(super) fn enter_first_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(table) = self.level_table_of(tick.state(), ids, entity) else {
            return self.enlist_banner_if_due(tick, ids, entity, None, 0);
        };
        let target = table.created_at;
        let lists: Vec<ModifierId> = table
            .levels
            .iter()
            .take(target.saturating_add(1))
            .flat_map(|level| level.modifiers.iter().copied())
            .collect();
        let points = table.level(target).map_or(0, |level| level.required);
        if target > 0 {
            let base_rank = table.level(0).map_or(0, |level| level.rank);
            tick.state_mut().insert(
                ids.experience,
                entity,
                Experience {
                    points,
                    level: u16::try_from(target).unwrap_or(u16::MAX),
                    base_rank,
                    max_level: 0,
                },
            )?;
        }
        self.gain_lists(tick, ids, entity, &lists)?;
        self.recompute_bonuses(tick, ids, entity)?;
        self.enlist_banner_if_due(tick, ids, entity, None, target)?;
        if target > 0 {
            self.propagate_rank(tick, ids, entity)?;
        }
        Ok(())
    }

    fn enlist_banner_if_due(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        old: Option<usize>,
        new: usize,
    ) -> Result<()> {
        let world = tick.state();
        let Some(horde) = world.get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let Some(plan) = self
            .banners
            .get(usize::from(horde.formation))
            .copied()
            .flatten()
        else {
            return Ok(());
        };
        let min = usize::try_from(plan.min_level).unwrap_or(usize::MAX);
        if new < min || old.is_some_and(|old| old >= min) {
            return Ok(());
        }
        let (Some(owner), Some(body)) = (
            world.get(ids.owner, horde_entity).map(|owner| owner.0),
            world.get(ids.body, horde_entity).copied(),
        ) else {
            return Ok(());
        };
        let offset = plan.offset.minus(horde.centre);
        let facing = body.0.facing;
        let pose = mint_sage_formation::FormationBasis::of(facing).place(body.0.position, offset);
        let pose = crate::skirmish::standable(&self.passability, pose);
        let Some(spawn) = self
            .kinds
            .spawn(&self.combat, plan.kind, pose, facing, owner)
        else {
            return Ok(());
        };
        let banner = tick.state_mut().spawn()?;
        Self::insert_spawn(tick, ids, banner, &spawn, &self.combat)?;
        self.attach_upgrade_state(tick, ids, banner, Some(plan.kind))?;
        crate::horde::enlist_banner(tick, Self::horde_ids(ids), horde_entity, banner, offset)?;
        self.note_banner(tick, ids, horde_entity, banner, plan.kind)?;
        self.raise_member_to_horde(tick, ids, horde_entity, banner)
    }

    pub(super) fn award_kills(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kills: &[(Entity, Entity)],
    ) -> Result<()> {
        for &(victim, killer) in kills {
            let scored = tick
                .state_mut()
                .get_mut(ids.health, victim)
                .is_some_and(|health| health.0.score_kill());
            if scored {
                continue;
            }
            let world = tick.state();
            let owner = |entity: Entity| world.get(ids.owner, entity).map(|owner| owner.0);
            let (Some(killer_owner), Some(victim_owner)) = (owner(killer), owner(victim)) else {
                continue;
            };
            self.credit_hero_kill(tick, ids, killer, victim);
            let world = tick.state();
            if killer_owner == victim_owner {
                self.award_own_guys(tick, ids, killer)?;
                continue;
            }
            if !scores_kill(
                killer_owner == victim_owner,
                self.object_hostile(world, killer, victim),
                world.contains(ids.construction, victim),
            ) {
                continue;
            }
            let award = self
                .experience_of(world, ids, victim)
                .and_then(|(index, _)| self.level_table_of(world, ids, victim)?.level(index))
                .map_or(0, |level| level.award);
            self.note_carryover_kill(tick, ids, killer);
            self.note_wotr_kill(tick, ids, killer);
            self.award_skill_points(tick, ids, killer_owner, award)?;
            self.pay_bounty(tick, ids, killer, killer_owner, victim)?;
            let world = tick.state();
            let Some(receiver) = self.experience_receiver(world, ids, killer) else {
                continue;
            };
            let scalar =
                self.xp_scalar(world, ids, receiver) * self.level_scalar(world, ids, receiver);
            let gain = scaled(award, scalar);
            if gain > 0 {
                self.add_experience(tick, ids, receiver, gain)?;
            }
        }
        Ok(())
    }

    pub(super) fn award_income(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        deposits: &[(Entity, i64)],
    ) -> Result<()> {
        for &(depositor, deposited) in deposits {
            let world = tick.state();
            let Some(record) = world
                .get(ids.kind, depositor)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
            else {
                continue;
            };
            if record.income_gives_no_xp {
                continue;
            }
            let Some(receiver) = self.experience_receiver(world, ids, depositor) else {
                continue;
            };
            let scalar =
                self.xp_scalar(world, ids, receiver) * self.level_scalar(world, ids, receiver);
            let gain = scaled(u32::try_from(deposited).unwrap_or(0), scalar);
            if gain > 0 {
                self.add_experience(tick, ids, receiver, gain)?;
            }
        }
        Ok(())
    }

    pub(super) fn reward_trigger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        caster: Entity,
        experience: u32,
        skill_points: u32,
    ) -> Result<()> {
        let world = tick.state();
        if let Some(receiver) = self.experience_receiver(world, ids, caster) {
            let scalar =
                self.xp_scalar(world, ids, receiver) * self.level_scalar(world, ids, receiver);
            let gain = scaled(experience, scalar);
            if gain > 0 {
                diag!(
                    "ability: entity {} triggered for {gain} experience",
                    caster.index()
                );
                self.add_experience(tick, ids, receiver, gain)?;
            }
        }
        if let Some(player) = tick.state().get(ids.owner, caster).map(|owner| owner.0) {
            self.award_skill_points(tick, ids, player, skill_points)?;
        }
        Ok(())
    }

    pub(super) fn debug_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        selected: &[Entity],
        points: u32,
    ) -> Result<()> {
        let mut holders: Vec<Entity> = selected
            .iter()
            .filter_map(|entity| self.experience_receiver(tick.state(), ids, *entity))
            .collect();
        holders.sort_unstable_by_key(|entity| entity.index());
        holders.dedup();
        for holder in holders {
            let scalar = self.level_scalar(tick.state(), ids, holder);
            let gain = scaled(points, scalar);
            if gain > 0 {
                self.add_experience(tick, ids, holder, gain)?;
            }
        }
        Ok(())
    }

    fn award_own_guys(&self, tick: &mut Tick<'_>, ids: Ids, killer: Entity) -> Result<()> {
        let world = tick.state();
        let Some(receiver) = self.experience_receiver(world, ids, killer) else {
            return Ok(());
        };
        let award = self
            .experience_of(world, ids, receiver)
            .and_then(|(index, _)| self.level_table_of(world, ids, receiver)?.level(index))
            .map_or(0, |level| level.own_guys);
        let scalar = self.xp_scalar(world, ids, receiver) * self.level_scalar(world, ids, receiver);
        let gain = scaled(award, scalar);
        if gain > 0 {
            self.add_experience(tick, ids, receiver, gain)?;
        }
        Ok(())
    }

    fn pay_bounty(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killer: Entity,
        killer_owner: PlayerId,
        victim: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let Some(player) =
            crate::economy::player_entity(world, Self::economy_ids(ids), killer_owner)
        else {
            return Ok(());
        };
        let cash = world.get(ids.cash_bounty, player).map_or(0.0, |b| b.0);
        let own = world
            .get(ids.bonuses, killer)
            .and_then(|bonuses| bonuses.0.bounty)
            .unwrap_or(0.0);
        let fraction = crate::spell_state::bounty_fraction(own, cash);
        if fraction.is_nan() || fraction <= 0.0 {
            return Ok(());
        }
        let value = world
            .get(ids.kind, victim)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.bounty_value.or(entry.record.build_cost))
            .unwrap_or(0.0);
        let bounty = crate::spell_state::bounty_for(value, fraction);
        if bounty <= 0 {
            return Ok(());
        }
        if let Some(purse) = tick.state_mut().get_mut(ids.purse, player) {
            purse.0.deposit(bounty);
        }
        Ok(())
    }

    pub(super) fn experience_receiver(
        &self,
        world: &World,
        ids: Ids,
        killer: Entity,
    ) -> Option<Entity> {
        let receiver = self.sink_of(world, ids, killer);
        if !world.is_live(receiver) || world.contains(ids.doomed, receiver) {
            return None;
        }
        self.level_table_of(world, ids, receiver)?
            .trainable
            .then_some(receiver)
    }

    pub(super) fn sink_of(&self, world: &World, ids: Ids, entity: Entity) -> Entity {
        world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| {
                world.is_live(*horde)
                    && world.contains(ids.horde, *horde)
                    && self.level_table_of(world, ids, *horde).is_some()
            })
            .unwrap_or(entity)
    }

    pub(super) fn add_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        amount: u32,
    ) -> Result<()> {
        let Some((old, points)) = self.experience_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        let Some(table) = self.level_table_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        if let Some(cap) = tick
            .state()
            .get(ids.experience, entity)
            .map(|held| held.max_level)
            .filter(|cap| *cap > 0)
        {
            if table.level(old).is_some_and(|level| level.rank >= cap) {
                return Ok(());
            }
        }
        let points = points.saturating_add(amount);
        let new = table.index_for(points).max(old);
        self.set_experience(tick, ids, entity, points, old, new)
    }

    pub(super) fn grant_horde_pools(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        experience: u32,
    ) -> Result<bool> {
        let world = tick.state();
        let Some(members) = world.get(ids.horde, horde).map(|held| held.members.clone()) else {
            return Ok(false);
        };
        let living: Vec<(Entity, Kind)> = members
            .into_iter()
            .filter(|member| world.is_live(*member) && !world.contains(ids.doomed, *member))
            .filter_map(|member| world.get(ids.kind, member).map(|kind| (member, *kind)))
            .collect();
        if living.is_empty() {
            return Ok(false);
        }
        #[allow(clippy::cast_precision_loss)]
        let share = experience as f32 / living.len() as f32;
        for &(member, kind) in &living {
            let seed = self.pool_seed(tick.state(), ids, member);
            let Some(mut held) = tick.state().get(ids.horde, horde).cloned() else {
                return Ok(true);
            };
            let pool = pool_add(&mut held.pools, kind, seed, share);
            tick.state_mut().insert(ids.horde, horde, held)?;
            for &(other, of) in &living {
                if of == kind {
                    self.set_points_from_pool(tick, ids, other, pool)?;
                }
            }
            let horde_points = self.experience_of(tick.state(), ids, horde).map(|(_, p)| p);
            #[allow(clippy::cast_precision_loss)]
            if horde_points.is_some_and(|points| pool > points as f32) {
                self.set_points_from_pool(tick, ids, horde, pool)?;
            }
        }
        Ok(true)
    }

    pub(super) fn seat_from_pool(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        member: Entity,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, member).copied() else {
            return Ok(());
        };
        let seed = self.pool_seed(tick.state(), ids, member);
        let Some(mut held) = tick.state().get(ids.horde, horde).cloned() else {
            return Ok(());
        };
        let pool = pool_add(&mut held.pools, kind, seed, 0.0);
        tick.state_mut().insert(ids.horde, horde, held)?;
        self.set_points_from_pool(tick, ids, member, pool)
    }

    fn pool_seed(&self, world: &World, ids: Ids, member: Entity) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        self.level_table_of(world, ids, member)
            .and_then(|table| table.level(table.created_at))
            .map_or(1.0, |level| level.required as f32)
    }

    fn set_points_from_pool(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        pool: f32,
    ) -> Result<()> {
        let Some((old, _)) = self.experience_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        let Some(table) = self.level_table_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        if !table.trainable {
            return Ok(());
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let points = pool.max(0.0) as u32;
        let new = table.index_for(points).max(old);
        self.set_experience(tick, ids, entity, points, old, new)
    }

    pub(super) fn restore_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        points: u32,
    ) -> Result<()> {
        let Some((old, current)) = self.experience_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        let Some(table) = self.level_table_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        let points = points.max(current);
        let new = table.index_for(points).max(old);
        self.set_experience(tick, ids, entity, points, old, new)
    }

    pub(super) fn gain_levels(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        levels: u32,
        cap: Option<u32>,
    ) -> Result<()> {
        let receiver = self.sink_of(tick.state(), ids, entity);
        let Some((old, points)) = self.experience_of(tick.state(), ids, receiver) else {
            return Ok(());
        };
        let Some(table) = self.level_table_of(tick.state(), ids, receiver) else {
            return Ok(());
        };
        let step = usize::try_from(levels).unwrap_or(usize::MAX);
        let mut wanted = old.saturating_add(step).min(table.top());
        if let Some(cap) = cap {
            let capped = table
                .index_at_rank(u16::try_from(cap).unwrap_or(u16::MAX))
                .unwrap_or(0);
            wanted = wanted.min(capped);
        }
        if wanted <= old {
            return Ok(());
        }
        let needed = table.level(wanted).map_or(points, |level| level.required);
        self.set_experience(tick, ids, receiver, points.max(needed), old, wanted)
    }

    pub(super) fn set_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        points: u32,
        old: usize,
        new: usize,
    ) -> Result<()> {
        let base_rank = self.base_rank_of(tick.state(), ids, entity);
        let max_level = tick
            .state()
            .get(ids.experience, entity)
            .map_or(0, |held| held.max_level);
        tick.state_mut().insert(
            ids.experience,
            entity,
            Experience {
                points,
                level: u16::try_from(new).unwrap_or(u16::MAX),
                base_rank,
                max_level,
            },
        )?;
        if new != old {
            self.level_changed(tick, ids, entity, old, new)?;
        }
        Ok(())
    }

    fn level_changed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        old: usize,
        new: usize,
    ) -> Result<()> {
        let lists: Vec<ModifierId> = self
            .level_table_of(tick.state(), ids, entity)
            .map(|table| {
                table
                    .levels
                    .iter()
                    .skip(old.saturating_add(1))
                    .take(new.saturating_sub(old))
                    .flat_map(|level| level.modifiers.iter().copied())
                    .collect()
            })
            .unwrap_or_default();
        if old == 0 && new > 0 {
            self.note_vetted(tick, ids, entity);
        }
        self.gain_lists(tick, ids, entity, &lists)?;
        self.recompute_bonuses(tick, ids, entity)?;
        self.raise_veterancy_bonus(tick, ids, entity, new)?;
        if new > old {
            let emotion = self
                .level_table_of(tick.state(), ids, entity)
                .and_then(|table| table.level(new))
                .and_then(|level| level.emotion);
            if let Some(kind) = emotion {
                self.pulse_emotion(tick, ids, entity, kind, None, super::emotions::ONE_FRAME)?;
            }
        }
        self.enlist_banner_if_due(tick, ids, entity, Some(old), new)?;
        self.forward_horde_level(tick, ids, entity, old, new)?;
        self.offer_upgrades(tick, ids, entity)
    }

    fn raise_veterancy_bonus(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        level: usize,
    ) -> Result<()> {
        let old = tick
            .state()
            .get(ids.bonus_conditions, entity)
            .map_or_else(BonusConditions::default, |held| held.0);
        let new = old.after_level_change(level);
        if new == old {
            return Ok(());
        }
        if new.0 == 0 {
            tick.state_mut().remove(ids.bonus_conditions, entity);
        } else {
            tick.state_mut()
                .insert(ids.bonus_conditions, entity, WeaponBonusConditions(new))?;
        }
        Ok(())
    }

    pub(super) fn propagate_rank(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
    ) -> Result<()> {
        let Some(mut members) = tick
            .state()
            .get(ids.horde, horde)
            .map(|horde| horde.members.clone())
        else {
            return Ok(());
        };
        members.sort_unstable_by_key(|member| member.index());
        for member in members {
            self.raise_member_to_horde(tick, ids, horde, member)?;
        }
        Ok(())
    }

    pub(super) fn raise_member_to_horde(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        member: Entity,
    ) -> Result<()> {
        let world = tick.state();
        if !world.is_live(member) || world.contains(ids.doomed, member) {
            return Ok(());
        }
        let Some((horde_level, _)) = self.experience_of(world, ids, horde) else {
            return Ok(());
        };
        let (Some(table), Some((old, _))) = (
            self.level_table_of(world, ids, member),
            self.experience_of(world, ids, member),
        ) else {
            return Ok(());
        };
        let new = horde_level.min(table.top());
        if new <= old {
            return Ok(());
        }
        let points = table.level(new).map_or(0, |level| level.required);
        self.set_experience(tick, ids, member, points, old, new)
    }

    fn forward_horde_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        old: usize,
        new: usize,
    ) -> Result<()> {
        let Some(mut members) = tick
            .state()
            .get(ids.horde, horde)
            .map(|h| h.members.clone())
        else {
            return Ok(());
        };
        let lists: Vec<ModifierId> = if new > old {
            self.level_table_of(tick.state(), ids, horde)
                .map(|table| {
                    table
                        .levels
                        .iter()
                        .skip(old.saturating_add(1))
                        .take(new.saturating_sub(old))
                        .flat_map(|level| level.modifiers.iter().copied())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        members.sort_unstable_by_key(|member| member.index());
        for member in members {
            let world = tick.state();
            if !world.is_live(member) || world.contains(ids.doomed, member) {
                continue;
            }
            self.gain_lists(tick, ids, member, &lists)?;
            self.recompute_bonuses(tick, ids, member)?;
            self.offer_upgrades(tick, ids, member)?;
        }
        Ok(())
    }

    fn forwarded_level_lists(&self, world: &World, ids: Ids, entity: Entity) -> Vec<ModifierId> {
        let Some(horde) = world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| world.is_live(*horde) && world.contains(ids.horde, *horde))
        else {
            return Vec::new();
        };
        let (Some(table), Some((index, _))) = (
            self.level_table_of(world, ids, horde),
            self.experience_of(world, ids, horde),
        ) else {
            return Vec::new();
        };
        let own: Vec<ModifierId> = match (
            self.level_table_of(world, ids, entity),
            self.experience_of(world, ids, entity),
        ) {
            (Some(own), Some((at, _))) => own
                .levels
                .iter()
                .take(at.saturating_add(1))
                .flat_map(|level| level.modifiers.iter().copied())
                .collect(),
            _ => Vec::new(),
        };
        let mut out: Vec<ModifierId> = Vec::new();
        for id in table
            .levels
            .iter()
            .take(index.saturating_add(1))
            .flat_map(|level| level.modifiers.iter().copied())
        {
            if !own.contains(&id) && !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    pub(super) fn forwarded_level_upgrades(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&UpgradeMask> {
        let horde = world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| world.is_live(*horde) && world.contains(ids.horde, *horde))?;
        self.level_upgrades(world, ids, horde)
    }

    fn gain_lists(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        lists: &[ModifierId],
    ) -> Result<()> {
        if lists.is_empty() {
            return Ok(());
        }
        let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
            return Ok(());
        };
        let before = health;
        for id in lists {
            if let Some(list) = self.combat.modifiers.get(*id) {
                let (add, mult) = list.health_change();
                if add != 0.0 || mult != 1.0 {
                    health.0.apply_health_modifier(add, mult);
                }
            }
        }
        if health != before {
            tick.state_mut().insert(ids.health, entity, health)?;
        }
        Ok(())
    }

    pub(super) fn recompute_bonuses(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let mut total = Attributes::IDENTITY;
        let counts = |list: &mint_sage_combat::ModifierList| {
            !self.category_suppressed(world, ids, entity, list.category.as_deref())
        };
        if let (Some(table), Some((index, _))) = (
            self.level_table_of(world, ids, entity),
            self.experience_of(world, ids, entity),
        ) {
            for level in table.levels.iter().take(index.saturating_add(1)) {
                for id in &level.modifiers {
                    if let Some(list) = self.combat.modifiers.get(*id).filter(|l| counts(l)) {
                        total.fold(list);
                    }
                }
            }
        }
        for id in self.forwarded_level_lists(world, ids, entity) {
            if let Some(list) = self.combat.modifiers.get(id).filter(|l| counts(l)) {
                total.fold(list);
            }
        }
        if let Some(state) = self
            .modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
        {
            for name in &state.attribute_modifiers {
                let list = self
                    .combat
                    .modifiers
                    .id(name)
                    .and_then(|id| self.combat.modifiers.get(id))
                    .filter(|list| list.is_permanent() && counts(list));
                if let Some(list) = list {
                    total.fold(list);
                }
            }
        }
        if let Some(timed) = world.get(ids.timed_modifiers, entity) {
            for (id, _) in &timed.0 {
                if let Some(list) = self.combat.modifiers.get(*id).filter(|l| counts(l)) {
                    total.fold(list);
                }
            }
        }
        if let Some(list) = self.wotr_player_list(world, ids, entity) {
            total.fold(&list);
        }
        if let Some(list) = self
            .stance_list_of(world, ids, entity)
            .and_then(|id| self.combat.modifiers.get(id))
            .filter(|list| counts(list))
        {
            total.fold(list);
        }
        if let Some(list) = self
            .group_bonus_list_of(world, ids, entity)
            .and_then(|id| self.combat.modifiers.get(id))
            .filter(|list| counts(list))
        {
            total.fold(list);
        }
        let held = world.get(ids.bonuses, entity).map(|bonuses| bonuses.0);
        match (total.is_identity(), held) {
            (true, Some(_)) => {
                tick.state_mut().remove(ids.bonuses, entity);
            }
            (false, held) if held != Some(total) => {
                tick.state_mut()
                    .insert(ids.bonuses, entity, Bonuses(total))?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn attribute_modifier_gained(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        if let Some(id) = self.combat.modifiers.id(name).filter(|id| {
            self.combat
                .modifiers
                .get(*id)
                .is_some_and(|l| l.is_permanent())
        }) {
            self.gain_lists(tick, ids, entity, &[id])?;
        }
        self.recompute_bonuses(tick, ids, entity)
    }

    pub(crate) fn weapon_bonus(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &WeaponTemplate,
    ) -> WeaponBonus {
        compute_bonus(
            &self.combat.weapon_bonus,
            &template.bonus,
            Self::bonus_conditions(world, ids, entity),
        )
    }

    pub(crate) fn bonus_conditions(world: &World, ids: Ids, entity: Entity) -> BonusConditions {
        world
            .get(ids.bonus_conditions, entity)
            .map_or_else(BonusConditions::default, |held| held.0)
    }

    pub(crate) fn fire_rate(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &WeaponTemplate,
    ) -> f32 {
        let rate = world
            .get(ids.bonuses, entity)
            .map_or(1.0, |bonuses| bonuses.0.rate_of_fire);
        rate * self
            .weapon_bonus(world, ids, entity, template)
            .rate_of_fire()
    }

    pub(super) fn bonuses_of(&self, world: &World, ids: Ids, entity: Entity) -> Attributes {
        world
            .get(ids.bonuses, entity)
            .map_or(Attributes::IDENTITY, |bonuses| bonuses.0)
    }

    pub(crate) fn experience_census(&self, world: &World) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.experience) else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for slot in 0..self.players {
            let player = mint_sim::PlayerId::new(slot);
            let (mut gained, mut above, mut points) = (0usize, 0usize, 0u64);
            let mut top = 0u16;
            for (entity, held) in store.iter() {
                if world.get(ids.owner, entity).map(|owner| owner.0) != Some(player) {
                    continue;
                }
                gained = gained.saturating_add(1);
                points = points.saturating_add(u64::from(held.points));
                if held.level > 0 {
                    above = above.saturating_add(1);
                    top = top.max(self.rank_of(world, entity).unwrap_or(0));
                }
            }
            lines.push(format!(
                "experience: player {slot} has {gained} object(s) holding experience \
                 ({points} point(s) in all), {above} above their first level, highest rank {top}"
            ));
        }
        lines
    }
}

fn pool_add(pools: &mut Vec<(u16, f32)>, kind: Kind, seed: f32, add: f32) -> f32 {
    let at = match pools.binary_search_by_key(&kind.0, |(held, _)| *held) {
        Ok(at) => at,
        Err(at) => {
            pools.insert(at, (kind.0, seed));
            at
        }
    };
    pools.get_mut(at).map_or(seed, |(_, pool)| {
        *pool += add;
        *pool
    })
}
