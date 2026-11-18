use mint_movement::Facing;
use mint_sage_modules::effects::{Effect, Name};
use mint_sim::{Entity, LogicFrames, Result, StateHasher, Tick, World};

use super::map_scripts::ForcedModelCondition;
use super::{Ids, RotwkRules};
use crate::diag::diag;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CahPreview {
    pub(crate) target: Entity,
    pub(crate) facing: f32,
    pub(crate) upgrades: Vec<String>,
    pub(crate) conditions: String,
    pub(crate) examine: Option<ExamineRaise>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExamineRaise {
    pub(crate) condition: &'static str,
    pub(crate) frames: u32,
}

impl CahPreview {
    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        self.target.hash_into(hasher);
        hasher.write_f32(self.facing);
        hasher.write_usize(self.upgrades.len());
        for name in &self.upgrades {
            hasher.write_bytes(name.as_bytes());
        }
        hasher.write_bytes(self.conditions.as_bytes());
        if let Some(raise) = &self.examine {
            hasher.write_bytes(raise.condition.as_bytes());
            hasher.write_u32(raise.frames);
        }
    }
}

impl RotwkRules {
    pub(super) fn apply_cah_preview(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        command: &CahPreview,
    ) -> Result<()> {
        let target = command.target;
        if !tick.state().is_live(target) {
            return Ok(());
        }
        if let Some(body) = tick.state_mut().get_mut(ids.body, target) {
            body.0.facing = Facing::from_radians(command.facing);
        }
        for name in &command.upgrades {
            match self.upgrade_bits.bit(name) {
                Some(bit) if !self.upgrade_bits.is_player(bit) => {
                    self.grant_object_upgrade(tick, ids, target, bit)?;
                }
                _ => diag!("cah: preview upgrade {name} is not an object upgrade"),
            }
        }
        if command.conditions.is_empty() {
            tick.state_mut().remove(ids.forced_model_condition, target);
        } else {
            tick.state_mut().insert(
                ids.forced_model_condition,
                target,
                ForcedModelCondition {
                    name: command.conditions.clone(),
                    until: None,
                },
            )?;
        }
        if let Some(raise) = &command.examine {
            let ticks_per_frame = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
            let effect = Effect::TimedModelCondition {
                flag: Name::from(raise.condition),
                lasts: LogicFrames::from_frames(raise.frames.saturating_mul(ticks_per_frame)),
                only_if_absent: false,
            };
            self.apply_effects(tick, ids, vec![(target, effect)])?;
        }
        Ok(())
    }

    pub(crate) fn named_poses(&self, world: &World) -> Vec<(String, Entity, [f32; 2], f32)> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.script_name) else {
            return Vec::new();
        };
        store
            .iter()
            .filter(|(entity, _)| world.is_live(*entity))
            .filter_map(|(entity, tag)| {
                let body = world.get(ids.body, entity)?;
                let at = body.0.position;
                Some((tag.0.clone(), entity, [at.x, at.y], body.0.facing.radians()))
            })
            .collect()
    }

    pub(crate) fn position_of(&self, world: &World, entity: Entity) -> Option<[f32; 2]> {
        let ids = self.ids?;
        let at = world.get(ids.body, entity)?.0.position;
        Some([at.x, at.y])
    }
}
