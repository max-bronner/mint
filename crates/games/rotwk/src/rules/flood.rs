use mint_movement::{Facing, Vec2};
use mint_sage_combat::Point3;
use mint_sage_modules::FloodSpec;
use mint_sim::{Component, Entity, Frame, StateHasher};

use super::*;

const MAX_PATH_POINTS: u32 = 9_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FloodPath {
    source: Entity,
    points: Vec<Vec2>,
    index: u32,
    stepped: Frame,
}

impl Component for FloodPath {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.source.hash_into(hasher);
        hasher.write_usize(self.points.len());
        for point in &self.points {
            hasher.write_f32(point.x);
            hasher.write_f32(point.y);
        }
        hasher.write_u32(self.index);
        self.stepped.hash_into(hasher);
    }
}

fn facing_toward(points: &[Vec2], index: u32) -> Option<Facing> {
    let at = usize::try_from(index).ok()?;
    let here = points.get(at)?;
    let next = points.get(at.checked_add(1)?)?;
    Facing::from_vector(next.minus(*here))
}

impl RotwkRules {
    pub(super) fn make_flood(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        spec: &FloodSpec,
    ) -> Result<()> {
        let Some(body) = tick.state().get(ids.body, source).map(|body| body.0) else {
            return Ok(());
        };
        let angle = if spec.relative {
            spec.angle + body.facing.radians()
        } else {
            spec.angle
        };
        let (sin, cos) = mint_math::f32::sin_cos(angle);
        let centre = body.position;
        for member in &spec.members {
            let Some(kind) = self.kinds.kind(&member.template) else {
                diag!(
                    "flood: entity {} names {}, which is not a template - no member",
                    source.index(),
                    member.template
                );
                continue;
            };
            let control = member.control_points.map(|(x, y)| {
                Point3::new(
                    cos * x - sin * y + centre.x,
                    sin * x + cos * y + centre.y,
                    0.0,
                )
            });
            let length = mint_sage_combat::bezier_length(&control);
            let wanted = (length / member.speed).ceil();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let count = if wanted.is_finite() {
                wanted.clamp(2.0, MAX_PATH_POINTS as f32) as u32
            } else {
                2
            };
            let points: Vec<Vec2> = mint_sage_combat::bezier_points(&control, count)
                .into_iter()
                .map(|point| Vec2::new(point.x, point.y))
                .collect();
            let Some(&start) = points.first() else {
                continue;
            };
            let facing = facing_toward(&points, 0).unwrap_or(body.facing);
            let Some(made) = self.spawn_kind_placed(
                tick,
                ids,
                kind,
                super::spawning::Placement::Exact(start),
                facing,
                owner,
            )?
            else {
                continue;
            };
            diag!(
                "flood: entity {} makes {} entity {} at ({:.0}, {:.0}), {} point(s) to ({:.0}, {:.0})",
                source.index(),
                member.template,
                made.index(),
                start.x,
                start.y,
                points.len(),
                points.last().map_or(start.x, |p| p.x),
                points.last().map_or(start.y, |p| p.y)
            );
            let mut path = FloodPath {
                source,
                points,
                index: 0,
                stepped: tick.frame(),
            };
            if let Some(dead) = self.step_flood_member(tick, ids, made, &mut path)? {
                self.resolve_deaths(tick, ids, &[dead])?;
            } else {
                tick.state_mut().insert(ids.flood_path, made, path)?;
            }
        }
        Ok(())
    }

    pub(super) fn advance_floods(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
    ) -> Result<Vec<(Entity, f32)>> {
        let members: Vec<Entity> = tick
            .state()
            .store(ids.flood_path)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let now = tick.frame();
        let mut killed = Vec::new();
        for member in members {
            let Some(mut path) = tick.state().get(ids.flood_path, member).cloned() else {
                continue;
            };
            if path.stepped >= now || !tick.state().is_live(path.source) {
                continue;
            }
            path.stepped = now;
            match self.step_flood_member(tick, ids, member, &mut path)? {
                Some(dead) => {
                    tick.state_mut().remove(ids.flood_path, member);
                    killed.push(dead);
                }
                None => {
                    tick.state_mut().insert(ids.flood_path, member, path)?;
                }
            }
        }
        Ok(killed)
    }

    fn step_flood_member(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
        path: &mut FloodPath,
    ) -> Result<Option<(Entity, f32)>> {
        path.index = path.index.saturating_add(1);
        let Some(&at) = usize::try_from(path.index)
            .ok()
            .and_then(|index| path.points.get(index))
        else {
            return self.kill(tick, ids, member, None, crate::combat::DeathNaming::Normal);
        };
        if let Some(mut body) = tick.state().get(ids.body, member).copied() {
            body.0.position = at;
            if let Some(facing) = facing_toward(&path.points, path.index) {
                body.0.facing = facing;
            }
            tick.state_mut().insert(ids.body, member, body)?;
        }
        Ok(None)
    }
}
