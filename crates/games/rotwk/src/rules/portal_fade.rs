use mint_sim::{Component, Entity, Frame, LogicFrames, Result, StateHasher, Tick, World};

use super::emotions::TICKS_PER_FRAME;
use super::{Ids, RotwkRules};

const FADE: LogicFrames = LogicFrames::from_frames(5 * TICKS_PER_FRAME);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PortalFade {
    out: bool,
    since: Frame,
    from: f32,
}

impl Component for PortalFade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.out);
        hasher.write_u64(self.since.get());
        hasher.write_f32(self.from);
    }
}

impl PortalFade {
    pub(crate) fn opacity(&self, now: Frame) -> f32 {
        let target = if self.out { 0.0 } else { 1.0 };
        let span = u64::from(FADE.get()).max(1);
        let elapsed = now.get().saturating_sub(self.since.get()).min(span);
        #[allow(clippy::cast_precision_loss)]
        let progress = elapsed as f32 / span as f32;
        self.from + (target - self.from) * progress
    }

    fn finished_in(&self, now: Frame) -> bool {
        !self.out && now.get().saturating_sub(self.since.get()) >= u64::from(FADE.get())
    }
}

impl RotwkRules {
    fn on_fading_portal(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let flagged = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.ai.fade_on_portals);
        flagged
            && world.get(ids.path, entity).is_some_and(|path| {
                world
                    .get(ids.body, entity)
                    .and_then(|body| usize::try_from(body.0.waypoint).ok())
                    .and_then(|at| path.route.portals.get(at))
                    .is_some_and(Option::is_some)
            })
    }

    pub(super) fn step_portal_fades(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let mut candidates: Vec<Entity> = tick
            .state()
            .store(ids.path)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        if let Some(store) = tick.state().store(ids.portal_fade) {
            candidates.extend_from_slice(store.entities());
        }
        candidates.sort_by_key(|entity| entity.index());
        candidates.dedup();
        for entity in candidates {
            let on = self.on_fading_portal(tick.state(), ids, entity);
            let held = tick.state().get(ids.portal_fade, entity).copied();
            match (on, held) {
                (true, None) => {
                    let fade = PortalFade {
                        out: true,
                        since: now,
                        from: 1.0,
                    };
                    tick.state_mut().insert(ids.portal_fade, entity, fade)?;
                }
                (true, Some(fade)) if !fade.out => {
                    let fade = PortalFade {
                        out: true,
                        since: now,
                        from: fade.opacity(now),
                    };
                    tick.state_mut().insert(ids.portal_fade, entity, fade)?;
                }
                (false, Some(fade)) if fade.out => {
                    let fade = PortalFade {
                        out: false,
                        since: now,
                        from: fade.opacity(now),
                    };
                    tick.state_mut().insert(ids.portal_fade, entity, fade)?;
                }
                (false, Some(fade)) if fade.finished_in(now) => {
                    tick.state_mut().remove(ids.portal_fade, entity);
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn portal_fade_shadowless(&self, world: &World, entity: Entity) -> bool {
        let Some(ids) = self.ids else {
            return false;
        };
        let out = |holder: Entity| world.get(ids.portal_fade, holder).is_some_and(|f| f.out);
        out(entity) || world.get(ids.member, entity).is_some_and(|m| out(m.horde))
    }

    pub(crate) fn portal_fade_opacity(&self, world: &World, entity: Entity, now: Frame) -> f32 {
        let Some(ids) = self.ids else {
            return 1.0;
        };
        let of = |holder: Entity| {
            world
                .get(ids.portal_fade, holder)
                .map_or(1.0, |fade| fade.opacity(now))
        };
        let horde = world.get(ids.member, entity).map_or(1.0, |m| of(m.horde));
        of(entity).min(horde)
    }
}
