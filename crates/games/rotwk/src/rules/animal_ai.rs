use super::*;
use crate::combat::{Foe, TargetGrid};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AnimalOrigin(pub(crate) Vec2);

impl Component for AnimalOrigin {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0.x.to_bits());
        hasher.write_u32(self.0.y.to_bits());
    }
}

const WANDER_ANGLE_DRAWS: u32 = 31;

const FLEE_SCAN_KINDS: [&str; 4] = ["INFANTRY", "CAVALRY", "MONSTER", "MACHINE"];

impl RotwkRules {
    pub(super) fn step_animals(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let animals: Vec<Entity> = tick
            .state()
            .store(ids.animal_origin)
            .map_or_else(Vec::new, |store| store.entities().to_vec());
        for animal in animals {
            self.step_one_animal(tick, ids, animal, living, grid)?;
        }
        Ok(())
    }

    fn step_one_animal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        animal: Entity,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        if tick.state().contains(ids.emotion, animal) {
            let mut effects = Vec::new();
            let phase = tick.frame().get().wrapping_add(u64::from(animal.index()));
            let frame_due = phase % u64::from(super::emotions::TICKS_PER_FRAME) == 0;
            self.advance_emotion_state(tick, ids, animal, frame_due, &mut effects)?;
            return self.apply_effects(tick, ids, effects);
        }
        let (Some(origin), Some(body)) = (
            tick.state().get(ids.animal_origin, animal).map(|o| o.0),
            tick.state().get(ids.body, animal).map(|b| b.0),
        ) else {
            return Ok(());
        };
        let Some(wander) = tick
            .state()
            .get(ids.kind, animal)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| entry.record.ai.animal_wander)
        else {
            return Ok(());
        };
        let owner = tick.state().get(ids.owner, animal).map(|o| o.0);
        if wander.update_timer > 0 {
            let period = u64::from(u32::try_from(wander.update_timer).unwrap_or(1).max(1));
            if tick.frame().get() % period == 0 {
                if let Some(enemy) = self.animal_flee_target(
                    tick,
                    ids,
                    animal,
                    owner,
                    body.position,
                    &wander,
                    living,
                    grid,
                ) {
                    self.start_animal_flee(tick, ids, animal, enemy, wander.flee_distance)?;
                    return Ok(());
                }
            }
        }
        if tick.state().contains(ids.path, animal) {
            return Ok(());
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let percent = wander.wander_percentage.clamp(0.0, 100.0) as u32;
        let roll = tick.rng().below(101);
        if roll >= percent {
            return Ok(());
        }
        if let Some(lick) = self.nearest_salt_lick(tick, ids, body.position, wander.flee_range) {
            if lick.minus(body.position).length() > wander.max_wander_radius {
                self.walk_animal(tick, ids, animal, lick)?;
                return Ok(());
            }
        } else if origin.minus(body.position).length() > wander.max_wander_radius {
            self.walk_animal(tick, ids, animal, origin)?;
            return Ok(());
        }
        let angle =
            f32::from(i16::try_from(tick.rng().below(WANDER_ANGLE_DRAWS)).unwrap_or(0) - 15);
        let (sin_a, cos_a) = mint_math::f32::sin_cos(angle);
        let (sin_f, cos_f) = mint_math::f32::sin_cos(body.facing.radians());
        let direction = Vec2::new(cos_f + cos_a, sin_f + sin_a);
        let max_distance = wander.max_wander_distance.max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bound = (max_distance as u32).saturating_add(1);
        #[allow(clippy::cast_precision_loss)]
        let scale = tick.rng().below(bound) as f32;
        let destination = Vec2::new(
            body.position.x + direction.x * scale,
            body.position.y + direction.y * scale,
        );
        if self.passability.cell_at(destination).is_some() {
            self.walk_animal(tick, ids, animal, destination)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn animal_flee_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        animal: Entity,
        owner: Option<PlayerId>,
        position: Vec2,
        wander: &mint_sage_modules::AnimalWander,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Option<Entity> {
        let range = wander.flee_range.max(0.0);
        let passing = |index: u32| -> Option<(f32, Entity)> {
            let at = usize::try_from(index).ok()?;
            let (foe, _) = living.get(at)?;
            if foe.entity == animal {
                return None;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            let reach = dx * dx + dy * dy;
            if reach > range * range {
                return None;
            }
            let kind = tick.state().get(ids.kind, foe.entity).copied()?;
            let entry = self.kinds.get(kind)?;
            if !FLEE_SCAN_KINDS
                .iter()
                .any(|name| entry.record.kind_of.iter().any(|k| k == name))
            {
                return None;
            }
            let allied = owner.is_some() && self.object_allied(tick.state(), animal, foe.entity);
            if allied {
                return None;
            }
            Some((reach, foe.entity))
        };
        super::auto_cast::nearest_by_expanding_scan(position, grid, 0.0, range, 0, passing)
            .flatten()
    }

    fn nearest_salt_lick(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        position: Vec2,
        range: f32,
    ) -> Option<Vec2> {
        let range = range.max(0.0);
        let store = tick.state().store(ids.kind)?;
        let mut best: Option<(f32, Vec2)> = None;
        for (entity, kind) in store.iter() {
            if !self
                .kinds
                .get(*kind)
                .is_some_and(|entry| entry.record.kind_of.iter().any(|k| k == "SALT_LICK"))
            {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity) else {
                continue;
            };
            let reach = body.0.position.minus(position).length();
            if reach > range {
                continue;
            }
            if best.is_none_or(|(best_reach, _)| reach < best_reach) {
                best = Some((reach, body.0.position));
            }
        }
        best.map(|(_, at)| at)
    }

    pub(super) fn start_animal_flee(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        animal: Entity,
        enemy: Entity,
        flee_distance: f32,
    ) -> Result<()> {
        let emotion = super::emotions::Emotion {
            nugget: 0,
            kind: 0,
            cause: Some(enemy),
            activated: tick.frame(),
            ends: mint_sim::Frame::FOREVER,
            lock_until: mint_sim::Frame::FOREVER,
            prevents_commands: false,
            attribute_applied: false,
            state: None,
        };
        tick.state_mut().insert(ids.emotion, animal, emotion)?;
        let mut effects = Vec::new();
        self.enter_emotion_state(
            tick,
            ids,
            animal,
            mint_sage_ini::EmotionAiState::RunAwayPanic,
            enemy,
            flee_distance.max(0.0),
            &mut effects,
        )?;
        self.apply_effects(tick, ids, effects)
    }

    fn walk_animal(&self, tick: &mut Tick<'_>, ids: Ids, animal: Entity, to: Vec2) -> Result<()> {
        let to = self.adjust_to_possible_destination(tick, ids, animal, to);
        crate::unit::give_path(
            tick.state_mut(),
            (ids.body, ids.path),
            animal,
            Path {
                route: Route::to_point(to),
                goal: to,
                kind: PathKind::Animal,
                facing: None,
            },
        )?;
        Ok(())
    }
}
