use super::*;
use mint_sage_modules::{ContactLabels, ContactPoint, ContactTags};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ContactClaim {
    pub(crate) target: Entity,
    pub(crate) point: u16,
    pub(crate) spot: Vec2,
    pub(crate) seen: Frame,
}

impl Component for ContactClaim {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.target.hash_into(hasher);
        hasher.write_u16(self.point);
        hasher.write_f32(self.spot.x);
        hasher.write_f32(self.spot.y);
        self.seen.hash_into(hasher);
    }
}

impl ContactClaim {
    fn live(&self, now: Frame) -> bool {
        self.seen.get().saturating_add(1) >= now.get()
    }
}

const SPECIAL_ATTACK_TAGS: [ContactTags; 4] = [
    ContactTags::SWOOP,
    ContactTags::GRAB,
    ContactTags::RAM,
    ContactTags::BOMB,
];

fn world_point(point: &ContactPoint, centre: Vec2, facing: Facing) -> Vec2 {
    let turn = facing.to_vector();
    let [x, y, _] = point.position;
    Vec2::new(
        centre.x + x * turn.x - y * turn.y,
        centre.y + x * turn.y + y * turn.x,
    )
}

pub(super) fn build_spot(
    points: &[ContactPoint],
    labels: &ContactLabels,
    (centre, facing, major_radius): (Vec2, Facing, f32),
    from: Vec2,
) -> Option<Vec2> {
    let bias = from
        .minus(centre)
        .normalised()
        .map_or(Vec2::ZERO, |towards| towards.scale(major_radius / 2.0));
    let turn = facing.to_vector();
    let local = [
        bias.x * turn.x + bias.y * turn.y,
        bias.y * turn.x - bias.x * turn.y,
        0.0,
    ];
    mint_sage_modules::nearest_labelled_point(points, labels, local)
        .map(|point| world_point(point, centre, facing))
}

pub(super) fn nearest_labelled(
    points: &[ContactPoint],
    labels: ContactTags,
    centre: Vec2,
    facing: Facing,
    from: Vec2,
) -> Option<(Vec2, f32)> {
    points
        .iter()
        .filter(|point| {
            SPECIAL_ATTACK_TAGS
                .iter()
                .any(|tag| labels.contains(*tag) && point.tags.contains(*tag))
        })
        .map(|point| (world_point(point, centre, facing), point.position[2]))
        .fold(None, |best: Option<(f32, (Vec2, f32))>, at| {
            let d = distance_squared(at.0, from);
            match best {
                Some((best_d, _)) if best_d <= d => best,
                _ => Some((d, at)),
            }
        })
        .map(|(_, at)| at)
}

fn distance_squared(a: Vec2, b: Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}

impl RotwkRules {
    pub(super) fn contact_points_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> &[ContactPoint] {
        tick.state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map_or(&[], |entry| entry.record.contact_points.as_slice())
    }

    pub(super) fn claim_contact(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        me: &Foe,
        target: Foe,
        (min_range, max_range): (f32, f32),
    ) -> Result<Foe> {
        let now = tick.frame();
        let special = self
            .ai_table_of(tick, ids, attacker)
            .map_or(ContactTags::default(), |ai| {
                ai.special_contact_points.tags()
            });
        let points = self.contact_points_of(tick, ids, target.entity);
        let eligible =
            !points.is_empty() && !SPECIAL_ATTACK_TAGS.iter().any(|tag| special.contains(*tag));
        if !eligible {
            tick.state_mut().remove(ids.contact_claim, attacker);
            return Ok(target);
        }
        let Some(target_body) = tick.state().get(ids.body, target.entity).copied() else {
            return Ok(target);
        };
        let mover = self.mover_class(tick, ids, attacker);
        let spot_of = |tick: &Tick<'_>, point: &ContactPoint| {
            let at = world_point(point, target_body.0.position, target_body.0.facing);
            let cell = self.passable_cell_for(tick, ids, at, mover)?;
            Some(if self.passability.cell_at(at) == Some(cell) {
                at
            } else {
                self.passability.geometry.centre_of(cell)
            })
        };
        let held = tick
            .state()
            .get(ids.contact_claim, attacker)
            .copied()
            .filter(|claim| claim.target == target.entity);
        let chosen = if let Some(claim) = held {
            points
                .get(usize::from(claim.point))
                .and_then(|point| spot_of(tick, point))
                .map(|spot| (claim.point, spot))
        } else {
            let taken: Vec<u16> = tick
                .state()
                .store(ids.contact_claim)
                .map(|store| {
                    store
                        .iter()
                        .filter(|(holder, claim)| {
                            *holder != attacker && claim.target == target.entity && claim.live(now)
                        })
                        .map(|(_, claim)| claim.point)
                        .collect()
                })
                .unwrap_or_default();
            points
                .iter()
                .enumerate()
                .filter(|(_, point)| point.tags.is_empty())
                .filter_map(|(index, point)| Some((u16::try_from(index).ok()?, point)))
                .filter(|(index, _)| !taken.contains(index))
                .filter_map(|(index, point)| Some((index, spot_of(tick, point)?)))
                .filter(|(_, spot)| {
                    in_range(
                        *spot,
                        me.radius,
                        target.position,
                        target.radius,
                        min_range,
                        max_range,
                    )
                })
                .map(|(index, spot)| (distance_squared(spot, me.position), index, spot))
                .fold(None, |best: Option<(f32, u16, Vec2)>, next| match best {
                    Some(best) if best.0 <= next.0 => Some(best),
                    _ => Some(next),
                })
                .map(|(_, index, spot)| {
                    crate::diag::diag!(
                        "contact: entity {} takes point {} of entity {} at ({:.0}, {:.0})",
                        attacker.index(),
                        index,
                        target.entity.index(),
                        spot.x,
                        spot.y,
                    );
                    (index, spot)
                })
        };
        let Some((point, spot)) = chosen else {
            tick.state_mut().remove(ids.contact_claim, attacker);
            return Ok(target);
        };
        tick.state_mut().insert(
            ids.contact_claim,
            attacker,
            ContactClaim {
                target: target.entity,
                point,
                spot,
                seen: now,
            },
        )?;
        Ok(Foe {
            position: spot,
            radius: 0.0,
            ..target
        })
    }

    pub(super) fn diag_contact_engagement(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        at: Vec2,
        target: Entity,
    ) {
        if !crate::diag::wants("contact") || self.contact_points_of(tick, ids, target).is_empty() {
            return;
        }
        let Some(body) = tick.state().get(ids.body, target) else {
            return;
        };
        let turn = body.0.facing.to_vector();
        let (dx, dy) = (at.x - body.0.position.x, at.y - body.0.position.y);
        let point = tick
            .state()
            .get(ids.contact_claim, attacker)
            .filter(|claim| claim.target == target)
            .map_or_else(
                || "geometry".to_owned(),
                |claim| format!("point {}", claim.point),
            );
        crate::diag::diag!(
            "contact: entity {} engages entity {} from local ({:.0}, {:.0}) on {point}",
            attacker.index(),
            target.index(),
            dx * turn.x + dy * turn.y,
            dy * turn.x - dx * turn.y,
        );
    }
}
