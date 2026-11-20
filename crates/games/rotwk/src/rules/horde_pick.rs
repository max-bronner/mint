use super::*;

pub(super) const SELECT_JITTER_LOW: f32 = 0.66;
pub(super) const SELECT_JITTER_HIGH: f32 = 1.33;

impl RotwkRules {
    pub(super) fn pick_member(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        from: (Vec2, f32),
        shooter: Option<(&Foe, f32)>,
        candidates: &[Foe],
    ) -> Option<Foe> {
        let mut best: Option<(f32, Foe)> = None;
        for foe in candidates {
            let (dx, dy, dz) = (
                foe.position.x - from.0.x,
                foe.position.y - from.0.y,
                foe.z - from.1,
            );
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();
            let score = match shooter {
                Some((me, range)) => {
                    let limit = self.member_range_at(tick.state(), ids, me, foe, range);
                    if distance >= limit
                        || self.view_blocked(tick, ids, me.position, foe.position, foe.radius)
                    {
                        continue;
                    }
                    let draw = tick.rng().next_f32();
                    distance * (SELECT_JITTER_LOW + (SELECT_JITTER_HIGH - SELECT_JITTER_LOW) * draw)
                }
                None => distance,
            };
            if best.is_none_or(|(held, _)| score < held) {
                best = Some((score, *foe));
            }
        }
        best.map(|(_, foe)| foe)
    }

    pub(super) fn horde_member_pick(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        found: Entity,
    ) -> Entity {
        let Some(horde) = tick.state().get(ids.member, found).map(|entry| entry.horde) else {
            return found;
        };
        let Some(members) = tick
            .state()
            .get(ids.horde, horde)
            .map(|horde| horde.members.clone())
        else {
            return found;
        };
        let candidates: Vec<Foe> = members
            .iter()
            .filter_map(|member| self.foe_of(tick, ids, *member))
            .filter(|foe| !foe.unattackable)
            .collect();
        let Some(body) = tick.state().get(ids.body, attacker).map(|body| body.0) else {
            return found;
        };
        let from = (body.position, Self::feet(tick.state(), ids, attacker));
        let me = (!tick.state().contains(ids.horde, attacker))
            .then(|| self.foe_of(tick, ids, attacker))
            .flatten();
        let shooter = me.as_ref().and_then(|me| {
            let template = tick
                .state()
                .get(ids.armed, attacker)
                .and_then(|armed| self.combat.weapons.get(armed.weapon))
                .filter(|template| !template.melee)?;
            Some((
                me,
                self.attack_range_of(tick.state(), ids, attacker, template),
            ))
        });
        self.pick_member(tick, ids, from, shooter, &candidates)
            .or_else(|| candidates.first().copied())
            .map_or(found, |foe| foe.entity)
    }
}
