use super::*;
use crate::combat::{OrderQueue, QueuedLeg};

const ORDER_QUEUE_ARRIVAL_RADIUS: f32 = 20.0;
const ORDER_QUEUE_IDLE_LIMIT: u8 = 2;

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
                }),
                None => world.get(ids.path, entity).map(|path| QueuedLeg {
                    point: path.goal,
                    attack: false,
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
        let (Some(current), Some(body)) =
            (queue.current, tick.state().get(ids.body, entity).copied())
        else {
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
        let Some(next) = queue.queued.first().copied() else {
            tick.state_mut().remove(ids.order_queue, entity);
            return Ok(());
        };
        let mut remaining = queue.queued.clone();
        remaining.remove(0);
        diag!(
            "order: queue entity {} leg done, next ({:.1}, {:.1}){} - {} leg(s) left",
            entity.index(),
            next.point.x,
            next.point.y,
            if next.attack { " attack-move" } else { "" },
            remaining.len()
        );
        self.start_queued_leg(tick, ids, entity, body.0.position, next, budgets)?;
        tick.state_mut().insert(
            ids.order_queue,
            entity,
            OrderQueue {
                current: Some(next),
                queued: remaining,
                idle_checks: 0,
            },
        )?;
        Ok(())
    }

    fn start_queued_leg(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        from: Vec2,
        leg: QueuedLeg,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        tick.state_mut().remove(ids.attack_order, entity);
        tick.state_mut().remove(ids.horde_target, entity);
        tick.state_mut().remove(ids.attack_move_order, entity);
        tick.state_mut().remove(ids.path, entity);
        Self::clear_path_retry(tick, ids, entity);
        if leg.attack {
            tick.state_mut().insert(
                ids.attack_move_order,
                entity,
                AttackMoveOrder {
                    destination: leg.point,
                    facing: None,
                },
            )?;
        } else {
            self.walk_toward_ordered(tick, ids, entity, from, leg.point, budgets)?;
        }
        Ok(())
    }
}
