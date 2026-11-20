use mint_movement::Vec2;
use mint_sim::{Entity, World};

use super::{Ids, RotwkRules};
use crate::unit::Selected;
use mint_sim::PlayerId;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueueStrip {
    pub(crate) points: Vec<Vec2>,
    pub(crate) loop_from: Option<usize>,
}

impl RotwkRules {
    pub(crate) fn order_queue_strip(&self, world: &World, entity: Entity) -> Option<QueueStrip> {
        let ids = self.ids?;
        let queue = world.get(ids.order_queue, entity)?;
        let at = world.get(ids.body, entity)?.0.position;
        let mut points = vec![at];
        points.extend(queue.current.iter().map(|leg| leg.point));
        points.extend(queue.queued.iter().map(|leg| leg.point));
        if points.len() < 3 && queue.loop_start.is_none() {
            return None;
        }
        Some(QueueStrip {
            points,
            loop_from: queue.loop_start.map(|start| start.saturating_add(1)),
        })
    }

    pub(crate) fn selected_queue_strips(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> Vec<QueueStrip> {
        let Some(store) = world.store(ids.selected) else {
            return Vec::new();
        };
        store
            .iter()
            .filter(|(_, selected): &(Entity, &Selected)| selected.holds(player))
            .filter_map(|(entity, _)| self.order_queue_strip(world, entity))
            .collect()
    }
}
