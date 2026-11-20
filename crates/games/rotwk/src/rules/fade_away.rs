use mint_sage_modules::Effect;
use mint_sim::{Component, Entity, LogicFrames, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FadeOpacity(pub(crate) f32);

impl Component for FadeOpacity {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

const BUSY_UNTIL_GONE: LogicFrames = LogicFrames::from_frames(30 * 3600);

impl RotwkRules {
    pub(super) fn fade_away(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        opacity: f32,
        first: bool,
    ) -> Result<()> {
        let member = tick.state().get(ids.member, entity).copied();
        if first && member.is_none() {
            self.apply_effects(
                tick,
                ids,
                vec![
                    (entity, Effect::AiBusy(BUSY_UNTIL_GONE)),
                    (
                        entity,
                        Effect::StatusBits {
                            set: mint_sage_modules::effects::names(vec!["UNSELECTABLE".to_owned()]),
                            clear: mint_sage_modules::effects::names(Vec::new()),
                        },
                    ),
                ],
            )?;
        }
        if opacity > 0.0 {
            tick.state_mut()
                .insert(ids.fade_opacity, entity, FadeOpacity(opacity))?;
            return Ok(());
        }
        if let Some(member) = member {
            if let Some(mut horde) = tick.state().get(ids.horde, member.horde).cloned() {
                horde.members.retain(|held| *held != entity);
                tick.state_mut().insert(ids.horde, member.horde, horde)?;
            }
        }
        let own_members = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        for held in own_members {
            tick.state_mut().despawn(held);
        }
        tick.state_mut().despawn(entity);
        Ok(())
    }
}
