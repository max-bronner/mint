use mint_sage_combat::{ModifierId, ModifierNugget};

use super::*;

impl RotwkRules {
    pub(super) fn apply_modifier_nugget(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        firer: Entity,
        victim: Entity,
        nugget: &ModifierNugget,
        list: ModifierId,
    ) -> Result<()> {
        if nugget.affect_horde_members {
            let members = tick
                .state()
                .get(ids.horde, victim)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for member in members {
                self.modifier_one(tick, ids, firer, member, nugget, list)?;
            }
        }
        self.modifier_one(tick, ids, firer, victim, nugget, list)
    }

    fn modifier_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        firer: Entity,
        victim: Entity,
        nugget: &ModifierNugget,
        list: ModifierId,
    ) -> Result<()> {
        if !self.inside_modifier_arc(tick.state(), ids, firer, victim, nugget.damage_arc) {
            return Ok(());
        }
        let now = tick.frame();
        let until = self.list_end(now, list);
        self.give_timed_list(tick, ids, victim, list, until)?;
        if until == Frame::FOREVER || nugget.anti_categories.is_empty() {
            return Ok(());
        }
        let mut suppressed = tick
            .state()
            .get(ids.suppressed, victim)
            .cloned()
            .unwrap_or_default();
        for category in &nugget.anti_categories {
            suppressed.add(category, until);
        }
        tick.state_mut()
            .insert(ids.suppressed, victim, suppressed)?;
        self.recompute_bonuses(tick, ids, victim)?;
        if let Some(fx) = &nugget.anti_fx {
            if let Some(at) = tick.state().get(ids.body, victim).map(|b| b.0.position) {
                let power = crate::fx::FxSource::Power;
                self.emit_fx_object_named(tick, ids, fx, at, power, victim, None);
            }
        }
        Ok(())
    }

    fn inside_modifier_arc(
        &self,
        world: &World,
        ids: Ids,
        firer: Entity,
        victim: Entity,
        arc: Option<f32>,
    ) -> bool {
        let Some(arc) = arc.filter(|arc| arc.is_finite() && *arc < 180.0) else {
            return true;
        };
        let (Some(from), Some(to)) = (world.get(ids.body, firer), world.get(ids.body, victim))
        else {
            return true;
        };
        let (dx, dy) = (
            to.0.position.x - from.0.position.x,
            to.0.position.y - from.0.position.y,
        );
        let dz = self.object_z(world, ids, victim) - self.object_z(world, ids, firer);
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        if !length.is_finite() || length <= 1e-4 {
            return true;
        }
        let forward = from.0.facing.to_vector();
        let dot = (dx * forward.x + dy * forward.y) / length;
        dot >= mint_math::f32::cos(arc.max(0.0).to_radians())
    }
}
