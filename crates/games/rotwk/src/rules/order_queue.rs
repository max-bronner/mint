use super::*;
use crate::combat::{OrderQueue, OrderTag, QueuedLeg};

const ORDER_QUEUE_ARRIVAL_RADIUS: f32 = 20.0;
const ORDER_QUEUE_IDLE_LIMIT: u8 = 2;

const LOOP_CLOSE_DISTANCE_SQ: f32 = 200.0;

impl RotwkRules {
    pub(super) fn order_queue_busy(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.order_queue, entity)
            .is_some_and(|queue| queue.current.is_some())
            || world.contains(ids.path, entity)
            || world.contains(ids.attack_move_order, entity)
    }

    pub(super) fn append_queued_leg(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        leg: QueuedLeg,
    ) -> Result<()> {
        let world = tick.state();
        let mut queue = world
            .get(ids.order_queue, entity)
            .cloned()
            .unwrap_or_default();
        if queue.current.is_none() {
            queue.current = match world.get(ids.attack_move_order, entity) {
                Some(order) => Some(QueuedLeg {
                    point: order.destination,
                    attack: true,
                    resolve: false,
                    tag: None,
                    non_final: order.non_final,
                    follow: order.follow,
                }),
                None => world.get(ids.path, entity).map(|path| QueuedLeg {
                    point: path.goal,
                    attack: false,
                    resolve: false,
                    tag: None,
                    non_final: false,
                    follow: false,
                }),
            };
        }
        queue.queued.push(leg);
        tick.state_mut().insert(ids.order_queue, entity, queue)?;
        Ok(())
    }

    pub(super) fn advance_order_queues(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let entities: Vec<Entity> = tick
            .state()
            .store(ids.order_queue)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in entities {
            self.advance_one_order_queue(tick, ids, entity, budgets)?;
        }
        Ok(())
    }

    fn advance_one_order_queue(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let Some(mut queue) = tick.state().get(ids.order_queue, entity).cloned() else {
            return Ok(());
        };
        let (Some(current), Some(body)) = (
            queue.current.clone(),
            tick.state().get(ids.body, entity).copied(),
        ) else {
            tick.state_mut().remove(ids.order_queue, entity);
            return Ok(());
        };
        let arrived = body.0.position.minus(current.point).length_squared()
            <= ORDER_QUEUE_ARRIVAL_RADIUS * ORDER_QUEUE_ARRIVAL_RADIUS;
        let idle = !tick.state().contains(ids.path, entity)
            && !tick.state().contains(ids.attack_move_order, entity);
        if arrived {
            queue.idle_checks = 0;
        } else if idle {
            queue.idle_checks = queue.idle_checks.saturating_add(1);
        } else {
            queue.idle_checks = 0;
        }
        if !arrived && queue.idle_checks < ORDER_QUEUE_IDLE_LIMIT {
            tick.state_mut().insert(ids.order_queue, entity, queue)?;
            return Ok(());
        }
        let mut remaining = queue.queued.clone();
        let loop_start = match queue.loop_start {
            Some(0) => {
                let lap = match &current.tag {
                    Some(tag) => Some(Self::lap_copy_of(tick, ids, tag)?),
                    None => None,
                };
                remaining.push(QueuedLeg {
                    point: current.tag.as_ref().map_or(current.point, |tag| tag.click),
                    attack: true,
                    resolve: current.tag.is_some(),
                    tag: lap,
                    non_final: false,
                    follow: false,
                });
                Some(0)
            }
            other => other.and_then(|at| at.checked_sub(1)),
        };
        if remaining.is_empty() {
            tick.state_mut().remove(ids.order_queue, entity);
            return Ok(());
        }
        let next = remaining.remove(0);
        diag!(
            "order: queue entity {} leg done, next ({:.1}, {:.1}){} - {} leg(s) left",
            entity.index(),
            next.point.x,
            next.point.y,
            if next.attack { " attack-move" } else { "" },
            remaining.len()
        );
        let next = self.start_queued_leg(tick, ids, entity, body.0.position, next, budgets)?;
        tick.state_mut().insert(
            ids.order_queue,
            entity,
            OrderQueue {
                current: Some(next),
                queued: remaining,
                idle_checks: 0,
                loop_start,
            },
        )?;
        Ok(())
    }

    fn lap_copy_of(tick: &mut Tick<'_>, ids: Ids, tag: &OrderTag) -> Result<OrderTag> {
        let id = match tag.lap_clone {
            Some(id) => id,
            None => {
                let id = Self::next_order_id(tick.state(), ids);
                Self::link_lap_clone(tick, ids, tag.id, id)?;
                id
            }
        };
        Ok(OrderTag {
            id,
            copy_of: Some(tag.id),
            lap_clone: None,
            ..tag.clone()
        })
    }

    fn link_lap_clone(tick: &mut Tick<'_>, ids: Ids, original: u32, clone: u32) -> Result<()> {
        let holders: Vec<Entity> = tick
            .state()
            .store(ids.order_queue)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for holder in holders {
            let Some(mut queue) = tick.state().get(ids.order_queue, holder).cloned() else {
                continue;
            };
            let mut changed = false;
            for leg in queue.current.iter_mut().chain(queue.queued.iter_mut()) {
                if let Some(held) = leg.tag.as_mut().filter(|held| held.id == original) {
                    held.lap_clone = Some(clone);
                    changed = true;
                }
            }
            if changed {
                tick.state_mut().insert(ids.order_queue, holder, queue)?;
            }
        }
        Ok(())
    }

    pub(super) fn next_order_id(world: &World, ids: Ids) -> u32 {
        world
            .store(ids.order_queue)
            .and_then(|store| {
                store
                    .iter()
                    .flat_map(|(_, queue)| queue.current.iter().chain(&queue.queued))
                    .filter_map(|leg| leg.tag.as_ref().map(|tag| tag.id))
                    .max()
            })
            .map_or(0, |id| id.saturating_add(1))
    }

    pub(super) fn link_waypoint_loop(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        tag: &OrderTag,
    ) -> Result<()> {
        let Some(start) = Self::loop_start(tick.state(), ids, tag) else {
            return Ok(());
        };
        for member in &tag.members {
            let Some(mut queue) = tick.state().get(ids.order_queue, *member).cloned() else {
                continue;
            };
            let legs = queue.current.iter_mut().chain(queue.queued.iter_mut());
            let mut at = None;
            for (index, leg) in legs.enumerate() {
                if at.is_none() && leg.tag.as_ref().is_some_and(|t| t.id == start) {
                    at = Some(index);
                }
                if at.is_some() {
                    leg.attack = true;
                }
            }
            if at.is_some() {
                queue.loop_start = at;
                tick.state_mut().insert(ids.order_queue, *member, queue)?;
            }
        }
        Ok(())
    }

    fn loop_start(world: &World, ids: Ids, tag: &OrderTag) -> Option<u32> {
        let near = |at: Vec2| at.minus(tag.click).length_squared() < LOOP_CLOSE_DISTANCE_SQ;
        let covers = |leg: &QueuedLeg| {
            leg.tag
                .as_ref()
                .is_some_and(|t| tag.members.iter().all(|m| t.members.contains(m)))
        };
        for member in &tag.members {
            let standing = world.is_live(*member)
                && world
                    .get(ids.body, *member)
                    .is_some_and(|body| near(body.0.position));
            if !standing {
                continue;
            }
            let front = world
                .get(ids.order_queue, *member)
                .and_then(|queue| queue.current.as_ref());
            if let Some(front) = front.filter(|leg| covers(leg)) {
                return front.tag.as_ref().map(|t| t.id);
            }
        }
        let queue = world.get(ids.order_queue, *tag.members.first()?)?;
        let legs: Vec<&QueuedLeg> = queue.current.iter().chain(&queue.queued).collect();
        for pair in legs.windows(2) {
            let [earlier, after] = pair else { continue };
            if earlier.tag.as_ref().is_some_and(|t| t.id == tag.id) {
                break;
            }
            if earlier.tag.as_ref().is_some_and(|t| near(t.click)) && covers(after) {
                return after.tag.as_ref().map(|t| t.id);
            }
        }
        None
    }

    fn start_queued_leg(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        mut leg: QueuedLeg,
        budgets: &mut PathBudgets,
    ) -> Result<QueuedLeg> {
        tick.state_mut().remove(ids.attack_order, entity);
        tick.state_mut().remove(ids.horde_target, entity);
        tick.state_mut().remove(ids.attack_move_order, entity);
        tick.state_mut().remove(ids.path, entity);
        Self::clear_path_retry(tick, ids, entity);
        if leg.resolve {
            let formations_on = self.modes(tick, ids).formation == FormationMode::On;
            if let Some((at, _)) = self
                .claim_in_member_order(tick, ids, &[entity], leg.point, formations_on)
                .get(&entity)
            {
                leg.point = *at;
            }
            leg.resolve = false;
        }
        if leg.attack {
            let order = if leg.follow {
                AttackMoveOrder::follow_leg(leg.point, leg.non_final)
            } else {
                AttackMoveOrder::new(leg.point, None, None)
            };
            tick.state_mut()
                .insert(ids.attack_move_order, entity, order)?;
        } else {
            self.walk_toward_ordered(tick, ids, entity, from, leg.point, budgets, !leg.non_final)?;
        }
        Ok(leg)
    }
}
