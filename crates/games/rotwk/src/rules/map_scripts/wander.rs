use std::collections::BTreeMap;

use mint_movement::Vec2;
use mint_sim::{Entity, Result, StateHasher, Tick};

use super::orders::ScriptOrder;
use super::MapScriptState;
use crate::rules::{Ids, RotwkRules};

const PATHFIND_CELL_SIZE: f32 = 10.0;

const DEFAULT_WANDER_CELLS: i32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Wanderers(BTreeMap<Entity, [u32; 2]>);

impl Wanderers {
    pub(super) fn begin(&mut self, entity: Entity, origin: Vec2) {
        self.0
            .insert(entity, [origin.x.to_bits(), origin.y.to_bits()]);
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.0.remove(&entity);
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity, _| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (entity, bits) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_u32(bits[0]);
            hasher.write_u32(bits[1]);
        }
    }
}

impl RotwkRules {
    fn wander_cells(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> i32 {
        let world = tick.state();
        let radius = world
            .get(ids.locomotion, entity)
            .map(|locomotion| locomotion.0.wander_radius)
            .or_else(|| {
                let horde = world.get(ids.horde, entity)?;
                let member = horde.members.first()?;
                world
                    .get(ids.locomotion, *member)
                    .map(|locomotion| locomotion.0.wander_radius)
            });
        radius.map_or(DEFAULT_WANDER_CELLS, |radius| {
            #[allow(clippy::cast_possible_truncation)]
            let cells = (radius / PATHFIND_CELL_SIZE + 0.5).floor() as i32;
            cells
        })
    }

    pub(super) fn step_wanderers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
    ) -> Result<()> {
        for (&entity, bits) in &state.rt.wanderers.0 {
            let world = tick.state();
            if !world.is_live(entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.path, entity)
                || world.contains(ids.attack_order, entity)
                || self.mood_asleep(world, ids, entity)
            {
                continue;
            }
            let cells = self.wander_cells(tick, ids, entity);
            if cells < 1 {
                continue;
            }
            let span = u32::try_from(cells.saturating_mul(2).saturating_add(1)).unwrap_or(1);
            let mut offset = [0.0_f32; 2];
            for axis in &mut offset {
                let drawn = i32::try_from(tick.rng().below(span)).unwrap_or(0);
                let steps = i16::try_from(drawn.saturating_sub(cells)).unwrap_or(0);
                *axis = f32::from(steps) * PATHFIND_CELL_SIZE;
            }
            let goal = Vec2::new(
                f32::from_bits(bits[0]) + offset[0],
                f32::from_bits(bits[1]) + offset[1],
            );
            self.issue_order_to(tick, ids, &[entity], ScriptOrder::Move(goal, false))?;
        }
        Ok(())
    }
}
