use mint_movement::Facing;
use mint_sage_ini::EmotionType;
use mint_sage_modules::register_modifiers::NotifyTargetsOfImminentProbableCrushingUpdate as Scan;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, overlaps, Foe, TargetGrid, Volume};
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CrushNotice(Frame);

impl Component for CrushNotice {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0.get());
    }
}

impl RotwkRules {
    pub(super) fn step_crush_notices(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let ticks = u64::from(mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        let now = tick.frame();
        if now.get() % ticks != 0 {
            return Ok(());
        }
        let mut due: Vec<Entity> = tick
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.crush_notice, *entity)
                    .is_none_or(|notice| now >= notice.0)
            })
            .collect();
        due.sort_unstable_by_key(|entity| entity.index());
        for entity in due {
            let Some((scan, horde)) = self.notice_scan_of(tick, ids, entity) else {
                continue;
            };
            let mut sleep = scan.time_between_updates();
            let crushers = if horde {
                self.members_nearest_first(tick, ids, entity)
            } else if tick.state().contains(ids.member, entity) {
                Vec::new()
            } else {
                vec![entity]
            };
            for crusher in crushers {
                let low = scan.time_between_updates().get();
                let jitter = tick.rng().below((low / 2).saturating_add(1));
                sleep = mint_sim::LogicFrames::from_frames(low.saturating_add(jitter));
                if self.notify_targets(tick, ids, crusher, &scan, living, grid)? {
                    break;
                }
            }
            let next = now.checked_add(sleep).unwrap_or(Frame::FOREVER);
            tick.state_mut()
                .insert(ids.crush_notice, entity, CrushNotice(next))?;
        }
        Ok(())
    }

    fn notice_scan_of(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<(Scan, bool)> {
        let record = self.record_of(tick, ids, entity)?;
        let (module, horde) = record.declared_modules.iter().rev().find_map(|module| {
            if module
                .name
                .eq_ignore_ascii_case("HordeNotifyTargetsOfImminentProbableCrushingUpdate")
            {
                Some((module, true))
            } else if module
                .name
                .eq_ignore_ascii_case("NotifyTargetsOfImminentProbableCrushingUpdate")
            {
                Some((module, false))
            } else {
                None
            }
        })?;
        let horde = horde && tick.state().contains(ids.horde, entity);
        mint_sage_modules::register_modifiers::notify_targets_of_imminent_probable_crushing_update_from_fields(
            &module.source(&self.upgrade_bits),
            Frame::ZERO,
        )
        .ok()
        .map(|scan| (scan, horde))
    }

    fn members_nearest_first(&self, tick: &Tick<'_>, ids: Ids, horde: Entity) -> Vec<Entity> {
        let world = tick.state();
        let Some(centre) = world.get(ids.body, horde).map(|body| body.0.position) else {
            return Vec::new();
        };
        let Some(members) = world.get(ids.horde, horde).map(|h| h.members.clone()) else {
            return Vec::new();
        };
        let mut ranked: Vec<(f32, Entity)> = members
            .into_iter()
            .filter_map(|member| {
                let at = world.get(ids.body, member)?.0.position;
                Some((at.minus(centre).length_squared(), member))
            })
            .collect();
        ranked.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index().cmp(&b.1.index())));
        ranked.into_iter().map(|(_, member)| member).collect()
    }

    fn notify_targets(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        crusher: Entity,
        scan: &Scan,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<bool> {
        let Some(body) = tick.state().get(ids.body, crusher).map(|body| body.0) else {
            return Ok(false);
        };
        let radius = tick
            .state()
            .get(ids.shape, crusher)
            .map_or(0.0, |shape| shape.radius);
        let state = self.crush_state(tick, ids, crusher, radius);
        if state.crusher_level == 0 || state.too_slow {
            return Ok(false);
        }
        let Some(&(me, _)) = living.iter().find(|(foe, _)| foe.entity == crusher) else {
            return Ok(false);
        };
        let seconds = scan.scan_ahead_time().as_seconds();
        let ahead = body.velocity().scale(seconds);
        let reach = ahead.length();
        if !reach.is_finite() || reach < 1.0 {
            return Ok(false);
        }
        let (length, depth) = match state.volume {
            Volume::Circle(r) => (2.0 * r, 2.0 * r),
            Volume::Box(a, b) => (2.0 * a, 2.0 * b),
        };
        let height = scan.scan_height().unwrap_or(3.0 * me.height);
        let width = scan.scan_width().unwrap_or(depth);
        let swept = Volume::Box((length + reach) / 2.0, width / 2.0);
        let centre = body.position.plus(ahead.scale(0.5));
        let facing = Facing::from_radians(mint_math::f32::atan2(ahead.y, ahead.x));
        let floor = me.z - height / 3.0;
        let mut candidates = Vec::new();
        grid.query_into(
            grid_cell(centre.x),
            grid_cell(centre.y),
            grid_radius_cells(swept.reach() + 1.0),
            &mut candidates,
        );
        candidates.sort_unstable();
        let mut targets = Vec::new();
        for at in candidates {
            let Some((victim, _)) = living.get(at as usize) else {
                continue;
            };
            if victim.entity == crusher
                || tick.state().contains(ids.doomed, victim.entity)
                || tick
                    .state()
                    .get(ids.health, victim.entity)
                    .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            let hostile = self.object_hostile(tick.state(), crusher, victim.entity);
            if !self.crush_pass_arm(tick, ids, crusher, hostile) {
                continue;
            }
            if victim.z > floor + height || floor > victim.z + victim.height {
                continue;
            }
            let theirs = self.crush_state(tick, ids, victim.entity, victim.radius);
            if i16::from(state.crusher_level) <= theirs.crushable_level {
                continue;
            }
            if overlaps(
                (centre, facing, swept),
                (victim.position, theirs.facing, theirs.volume),
            ) {
                targets.push(victim.entity);
            }
        }
        if targets.is_empty() {
            return Ok(false);
        }
        diag!(
            "crush notice: frame {} entity {} warns {} target(s)",
            tick.frame().get(),
            crusher.index(),
            targets.len()
        );
        self.pulse_emotion(
            tick,
            ids,
            crusher,
            EmotionType::CheerForAboutToCrush,
            None,
            LogicFrames::ZERO,
        )?;
        for target in targets {
            self.pulse_emotion(
                tick,
                ids,
                target,
                EmotionType::BraceForBeingCrushed,
                Some(crusher),
                LogicFrames::ZERO,
            )?;
        }
        Ok(true)
    }
}
