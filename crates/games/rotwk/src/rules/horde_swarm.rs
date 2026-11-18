use super::horde_melee::Snapshot;
use super::*;
use crate::goal_cells;
use crate::melee::{HordeMelee, MemberMelee, SwarmState};

const RINGS_PER_CELL: i32 = 2;

const FAR_PER_CELL: f32 = 40.0;

const EARLY_STOP_GAIN: f32 = core::f32::consts::FRAC_1_SQRT_2;

const SHORT_WAIT_GAIN: f32 = 0.5;
const SHORT_WAIT_MS: u32 = 2000;

const FAIL_WAIT_MS: u32 = 4000;

const SEARCH_HEIGHT: f32 = 40.0;

const ROTATE_ABOVE: f32 = core::f32::consts::FRAC_PI_2;

const ROTATED_WITHIN: f32 = core::f32::consts::PI / 18.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LeaderTrail(pub(crate) Vec<Vec2>);

impl Component for LeaderTrail {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for crumb in &self.0 {
            hasher.write_f32(crumb.x);
            hasher.write_f32(crumb.y);
        }
    }
}

struct Pass<'a> {
    enemies: &'a [Foe],
    goal: Entity,
    loose_goal: Option<Foe>,
    force: bool,
    snapshot: &'a Snapshot<'a>,
    now: Frame,
}

impl RotwkRules {
    pub(super) fn is_siege_tower(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        tick.state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.kind_of.iter().any(|k| k == "SIEGE_TOWER"))
    }

    fn melee_reach(&self, tick: &Tick<'_>, ids: Ids, member: Entity) -> Option<f32> {
        let armed = tick.state().get(ids.armed, member)?;
        let template = self.combat.weapons.get(armed.weapon).filter(|t| t.melee)?;
        Some(self.attack_range_of(tick.state(), ids, member, template))
    }

    fn claim_standing_at(&self, tick: &mut Tick<'_>, ids: Ids, member: Entity, at: Vec2) {
        let footprint = self.footprint_of(tick, ids, member);
        let geometry = self.passability.geometry;
        let cell = goal_cells::anchor(geometry, at, footprint);
        let Some(rect) = goal_cells::rect(self.passability.grid.extent(), cell, footprint) else {
            return;
        };
        if let Some(layer) = goal_cells::layer_mut(tick, ids.goal_cells) {
            layer.claim(member, rect);
        }
    }

    pub(super) fn touching_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        goal: Entity,
        snapshot: &Snapshot<'_>,
    ) -> Result<bool> {
        if tick.state().contains(ids.member_attack, member.entity) {
            return Ok(true);
        }
        if self.melee_reach(tick, ids, member.entity).is_none() {
            return Ok(false);
        }
        let Some(_) = tick.state().get(ids.owner, member.entity).map(|o| o.0) else {
            return Ok(false);
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let mine = self.footprint_of(tick, ids, member.entity);
        let Some(mine_rect) = goal_cells::rect(
            extent,
            goal_cells::anchor(geometry, member.position, mine),
            mine,
        ) else {
            return Ok(false);
        };
        let reach = geometry.cell_size() * (f32::from(mine.side()) + 2.0);
        let mut found: Vec<u32> = Vec::new();
        snapshot.grid.query_into(
            grid_cell(member.position.x),
            grid_cell(member.position.y),
            grid_radius_cells(TargetGrid::search_radius(
                reach,
                member.radius,
                snapshot.global_max_radius,
            )),
            &mut found,
        );
        found.sort_unstable();
        let mut best: Option<((u8, bool, f32), Foe)> = None;
        for at in found {
            #[allow(clippy::cast_possible_truncation)]
            let Some((foe, _)) = snapshot.living.get(at as usize).copied() else {
                continue;
            };
            if !self.object_hostile(tick.state(), member.entity, foe.entity)
                || foe.unattackable
                || tick.state().contains(ids.horde, foe.entity)
            {
                continue;
            }
            let theirs = self.footprint_of(tick, ids, foe.entity);
            let Some(their_rect) = goal_cells::rect(
                extent,
                goal_cells::anchor(geometry, foe.position, theirs),
                theirs,
            ) else {
                continue;
            };
            let tier = if mine_rect.overlaps(their_rect) {
                0
            } else if mine_rect.touches(their_rect) {
                1
            } else {
                continue;
            };
            let victim = foe.entity == goal
                || tick
                    .state()
                    .get(ids.member, foe.entity)
                    .is_some_and(|entry| entry.horde == goal);
            if !victim && (foe.is_structure || foe.not_autoacquirable) {
                continue;
            }
            let gap = foe.position.minus(member.position).length() - foe.radius;
            let key = (tier, !victim, gap);
            if best.is_none_or(|(held, _)| {
                key.0
                    .cmp(&held.0)
                    .then(key.1.cmp(&held.1))
                    .then(key.2.total_cmp(&held.2))
                    .is_lt()
            }) {
                best = Some((key, foe));
            }
        }
        let Some((_, enemy)) = best else {
            return Ok(false);
        };
        self.attack_unit(tick, ids, member, enemy.entity)?;
        Ok(true)
    }

    pub(super) fn hold_ground(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        melee_horde: Entity,
        horde: &Horde,
        melee: &HordeMelee,
        snapshot: &Snapshot<'_>,
    ) -> Result<()> {
        let enemies = self.goal_members(tick, ids, melee.goal);
        let poses = tick
            .state()
            .get(ids.body, melee_horde)
            .map(|body| crate::horde::slot_poses(horde, &body.0))
            .unwrap_or_default();
        for member in &horde.members {
            if tick.state().contains(ids.member_attack, *member) {
                continue;
            }
            let Some(foe) = self.foe_of(tick, ids, *member) else {
                continue;
            };
            let mut record = tick
                .state()
                .get(ids.member_melee, *member)
                .copied()
                .unwrap_or_else(|| MemberMelee::at(foe.position, 0));
            let face = Self::nearest_to(&enemies, foe.position)
                .map_or(record.face, |enemy| enemy.position);
            let place = tick
                .state()
                .get(ids.member, *member)
                .and_then(|entry| entry.slot)
                .and_then(|slot| poses.get(usize::from(slot)).copied())
                .unwrap_or(foe.position);
            if record.goal != place || record.face != face {
                record.goal = place;
                record.face = face;
                tick.state_mut().insert(ids.member_melee, *member, record)?;
            }
            self.touching_attack(tick, ids, &foe, melee.goal, snapshot)?;
        }
        Ok(())
    }

    pub(super) fn swarm(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        members: &[Entity],
        melee: &mut HordeMelee,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<()> {
        let enemies = self.goal_members(tick, ids, melee.goal);
        if enemies.is_empty() {
            return Ok(());
        }
        let loose_goal = if tick.state().contains(ids.horde, melee.goal) {
            None
        } else {
            self.foe_of(tick, ids, melee.goal)
        };
        let pass = Pass {
            enemies: &enemies,
            goal: melee.goal,
            loose_goal,
            force: melee.force,
            snapshot,
            now,
        };
        melee.force = false;
        let mut budget = (members.len() / 2).max(2);
        for member in members {
            if tick.state().contains(ids.path, *member) {
                continue;
            }
            let Some(foe) = self.foe_of(tick, ids, *member) else {
                continue;
            };
            let mut record = tick
                .state()
                .get(ids.member_melee, *member)
                .copied()
                .unwrap_or_else(|| MemberMelee::at(foe.position, 0));
            if let Some(nearest) = Self::nearest_to(pass.enemies, foe.position) {
                record.face = nearest.position;
            }
            self.arrive(tick, ids, &foe, &mut record)?;
            let spent = self.swarm_decide(tick, ids, &foe, &mut record, &pass, &mut budget)?;
            tick.state_mut().insert(ids.member_melee, *member, record)?;
            if spent {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: &Horde,
        melee: &mut HordeMelee,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<()> {
        for member in &horde.members {
            self.drop_breadcrumb(tick, ids, *member)?;
        }
        let mut leaderless = Vec::new();
        for member in &horde.members {
            match self.leader_of(tick, ids, horde, *member) {
                Some(leader) => {
                    self.follow_leader(tick, ids, *member, leader, melee.goal, snapshot, now)?;
                }
                None => leaderless.push(*member),
            }
        }
        if leaderless.is_empty() {
            return Ok(());
        }
        self.swarm(tick, ids, &leaderless, melee, snapshot, now)
    }

    fn leader_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde: &Horde,
        member: Entity,
    ) -> Option<Entity> {
        let own_slot = tick.state().get(ids.member, member)?.slot?;
        let leader_ref = horde.slots.get(usize::from(own_slot))?.leader?;
        horde.members.iter().copied().find(|candidate| {
            tick.state()
                .get(ids.member, *candidate)
                .and_then(|entry| entry.slot)
                .is_some_and(|slot| {
                    horde.slots.get(usize::from(slot)).is_some_and(|slot| {
                        slot.rank == leader_ref.rank && slot.index_in_rank == leader_ref.index
                    })
                })
        })
    }

    const BREADCRUMB_SPACING: f32 = 10.0;

    const BREADCRUMB_DEPTH: usize = 10;

    const NO_BREADCRUMB_WAIT_FRAMES: u32 = SIMULATION_HZ;

    fn drop_breadcrumb(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        let position = body.0.position;
        let mut trail = tick
            .state()
            .get(ids.leader_trail, entity)
            .cloned()
            .unwrap_or_default();
        let drop = trail
            .0
            .first()
            .is_none_or(|last| last.minus(position).length() >= Self::BREADCRUMB_SPACING);
        if !drop {
            return Ok(());
        }
        trail.0.insert(0, position);
        trail.0.truncate(Self::BREADCRUMB_DEPTH);
        tick.state_mut().insert(ids.leader_trail, entity, trail)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn follow_leader(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
        leader: Entity,
        goal: Entity,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<()> {
        if tick.state().contains(ids.member_attack, member) {
            return Ok(());
        }
        let Some(foe) = self.foe_of(tick, ids, member) else {
            return Ok(());
        };
        let mut record = tick
            .state()
            .get(ids.member_melee, member)
            .copied()
            .unwrap_or_else(|| MemberMelee::at(foe.position, 0));
        if record.wait_until.is_some_and(|until| until > now) {
            tick.state_mut().insert(ids.member_melee, member, record)?;
            return Ok(());
        }
        let crumb = tick
            .state()
            .get(ids.leader_trail, leader)
            .and_then(|trail| trail.0.last().copied());
        let Some(target) = crumb else {
            record.wait_until = Some(Self::frame_after(now, Self::NO_BREADCRUMB_WAIT_FRAMES));
            tick.state_mut().insert(ids.member_melee, member, record)?;
            return Ok(());
        };
        if record.state == SwarmState::None || record.state == SwarmState::Arrived {
            record.goal = target;
            record.state = SwarmState::Moving;
        }
        if let Some(leader_body) = tick.state().get(ids.body, leader) {
            record.face = leader_body.0.position;
        }
        self.arrive(tick, ids, &foe, &mut record)?;
        self.touching_attack(tick, ids, &foe, goal, snapshot)?;
        tick.state_mut().insert(ids.member_melee, member, record)?;
        Ok(())
    }

    fn arrive(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        record: &mut MemberMelee,
    ) -> Result<()> {
        if !matches!(record.state, SwarmState::Moving | SwarmState::Rotating) {
            return Ok(());
        }
        let Some(mut body) = tick.state().get(ids.body, member.entity).copied() else {
            return Ok(());
        };
        if record.state == SwarmState::Moving {
            let limits = self.mover_limits(tick, ids, member.entity);
            let reach = crate::horde::arrive(&limits, Self::tick_seconds());
            if record.goal.minus(body.0.position).length() >= reach {
                return Ok(());
            }
            body.0.position = record.goal;
            tick.state_mut().insert(ids.body, member.entity, body)?;
        }
        let turn = Facing::from_vector(record.face.minus(body.0.position))
            .map_or(0.0, |towards| body.0.facing.delta_to(towards).abs());
        record.state = match record.state {
            SwarmState::Moving if turn > ROTATE_ABOVE => SwarmState::Rotating,
            SwarmState::Rotating if turn > ROTATED_WITHIN => SwarmState::Rotating,
            _ => SwarmState::Arrived,
        };
        Ok(())
    }

    fn swarm_decide(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        record: &mut MemberMelee,
        pass: &Pass<'_>,
        budget: &mut usize,
    ) -> Result<bool> {
        if tick.state().contains(ids.member_attack, member.entity) {
            record.dirty = true;
            record.goal = member.position;
            self.claim_standing_at(tick, ids, member.entity, member.position);
            return Ok(false);
        }
        if let Some(goal) = pass.loose_goal {
            let mut direct =
                record.state == SwarmState::Arrived && self.is_siege_tower(tick, ids, goal.entity);
            if goal.is_structure {
                let crowd = self.claimants_at(tick, ids, member, member.position);
                direct |= crowd == 0 || crowd == 2;
            }
            let reaches = self
                .melee_reach(tick, ids, member.entity)
                .is_some_and(|range| {
                    in_range(
                        member.position,
                        member.radius,
                        goal.position,
                        goal.radius,
                        0.0,
                        range,
                    )
                });
            if direct && reaches {
                self.attack_unit(tick, ids, member, goal.entity)?;
                record.goal = member.position;
                return Ok(false);
            }
        }
        if record.wait_until.is_some_and(|until| until > pass.now) && !pass.force {
            return Ok(false);
        }
        if matches!(record.state, SwarmState::Moving | SwarmState::Rotating) {
            return Ok(false);
        }
        if record.state == SwarmState::Arrived {
            self.touching_attack(tick, ids, member, pass.goal, pass.snapshot)?;
            if tick.state().contains(ids.member_attack, member.entity) {
                return Ok(false);
            }
        }
        if record.state == SwarmState::None && *budget > 0 {
            record.dirty = true;
        }
        if !record.dirty && record.state != SwarmState::Arrived {
            return Ok(false);
        }
        record.goal = member.position;
        let Some(target) = Self::nearest_to(pass.enemies, record.goal) else {
            return Ok(false);
        };
        let (found, wait) = self.find_melee_position(tick, ids, member, target.position);
        record.wait_until = wait.map(|ms| Self::after(pass.now, ms));
        if let Some(at) = found {
            record.goal = at;
            record.state = SwarmState::Moving;
            *budget = budget.saturating_sub(1);
        } else {
            if record.wait_until.is_some() {
                self.touching_attack(tick, ids, member, pass.goal, pass.snapshot)?;
            }
            record.state = SwarmState::None;
        }
        self.claim_standing_at(tick, ids, member.entity, record.goal);
        record.dirty = false;
        Ok(*budget < 1)
    }

    fn claimants_at(&self, tick: &Tick<'_>, ids: Ids, member: &Foe, at: Vec2) -> usize {
        let footprint = self.footprint_of(tick, ids, member.entity);
        let cell = goal_cells::anchor(self.passability.geometry, at, footprint);
        goal_cells::rect(self.passability.grid.extent(), cell, footprint)
            .zip(goal_cells::layer(tick.state(), ids.goal_cells))
            .map_or(0, |(rect, layer)| layer.claimants(member.entity, rect))
    }

    fn find_melee_position(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        member: &Foe,
        target: Vec2,
    ) -> (Option<Vec2>, Option<u32>) {
        let footprint = self.footprint_of(tick, ids, member.entity);
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let here = goal_cells::anchor(geometry, member.position, footprint);
        let start = goal_cells::anchor_point(geometry, here, footprint);
        let current = target.minus(start).length();
        let side = i32::from(footprint.side());
        #[allow(clippy::cast_precision_loss)]
        let far = (FAR_PER_CELL * side as f32) < current;
        let layer = goal_cells::layer(tick.state(), ids.goal_cells);
        let crowded = goal_cells::rect(extent, here, footprint)
            .zip(layer)
            .is_some_and(|(rect, layer)| layer.claimants(member.entity, rect) > 0);
        let mover = self.mover_class(tick, ids, member.entity);
        let view = self.dynamic_view(tick, ids);
        let passable = |at: Vec2| {
            geometry
                .cell_at(at, 0, extent)
                .is_some_and(|cell| view.is_passable(cell, mover))
        };
        let line_passable = |to: Vec2| {
            let span = to.minus(start);
            let half_cell = geometry.cell_size() * 0.5;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let steps = (span.length() / half_cell).ceil().clamp(1.0, 64.0) as u32;
            (1..=steps).all(|step| {
                #[allow(clippy::cast_precision_loss)]
                let along = step as f32 / steps as f32;
                passable(start.plus(span.scale(along)))
            })
        };
        let mut best = current + 100.0;
        let mut chosen: Option<(Vec2, f32, f32)> = None;
        for ring in 1..=side.saturating_mul(RINGS_PER_CELL) {
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    if dx.abs().max(dy.abs()) != ring {
                        continue;
                    }
                    let cell = (here.0.saturating_add(dx), here.1.saturating_add(dy));
                    let to = goal_cells::anchor_point(geometry, cell, footprint);
                    let d = target.minus(to).length();
                    if (d > current && !crowded) || d >= best {
                        continue;
                    }
                    let Some(rect) = goal_cells::rect(extent, cell, footprint) else {
                        continue;
                    };
                    if self
                        .passability
                        .heights
                        .height_at_point(to)
                        .is_some_and(|ground| (ground - member.z).abs() > SEARCH_HEIGHT)
                    {
                        continue;
                    }
                    if !passable(to)
                        || !layer.is_none_or(|layer| layer.free_for(member.entity, rect))
                        || !line_passable(to)
                    {
                        continue;
                    }
                    best = d;
                    chosen = Some((to, current - d, to.minus(start).length()));
                }
            }
            if !far
                && chosen.is_some_and(|(_, gain, step)| step > 0.0 && gain / step > EARLY_STOP_GAIN)
            {
                break;
            }
        }
        match chosen {
            Some((to, gain, step)) => (
                Some(to),
                (step > 0.0 && gain / step < SHORT_WAIT_GAIN).then_some(SHORT_WAIT_MS),
            ),
            None => (None, Some(FAIL_WAIT_MS)),
        }
    }
}
