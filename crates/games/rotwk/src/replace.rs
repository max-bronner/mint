use mint_movement::{Facing, Vec2};
use mint_sage_modules::ReplaceCarry;
use mint_sim::{Entity, PlayerId, Result, Tick};

use crate::experience::Experience;
use crate::rules::Ids;

pub(crate) fn replace_object(
    tick: &mut Tick<'_>,
    ids: Ids,
    entity: Entity,
    carry: ReplaceCarry,
    spawn: impl FnOnce(&mut Tick<'_>, Vec2, Facing, PlayerId) -> Result<Option<Entity>>,
    level_for_points: impl FnOnce(u32) -> Option<u16>,
) -> Result<Option<Entity>> {
    let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
        return Ok(None);
    };
    let Some(body) = tick.state().get(ids.body, entity).map(|body| body.0) else {
        return Ok(None);
    };
    let health_fraction = carry
        .health_fraction
        .then(|| {
            tick.state()
                .get(ids.health, entity)
                .map(|health| health.0.fraction())
        })
        .flatten();
    let paid_cost = tick
        .state()
        .get(ids.object_value, entity)
        .map(|value| value.paid_cost);
    let experience = carry
        .experience
        .then(|| tick.state().get(ids.experience, entity).copied())
        .flatten();
    let selected = if carry.selection {
        tick.state().get(ids.selected, entity).copied()
    } else {
        None
    };

    let Some(replacement) = spawn(tick, body.position, body.facing, owner)? else {
        return Ok(None);
    };

    if let Some(fraction) = health_fraction {
        if let Some(mut health) = tick.state().get(ids.health, replacement).copied() {
            let max = health.0.max();
            health.0.set_current(max * fraction);
            tick.state_mut().insert(ids.health, replacement, health)?;
        }
    }
    if let Some(experience) = experience {
        let level = level_for_points(experience.points).unwrap_or(experience.level);
        tick.state_mut().insert(
            ids.experience,
            replacement,
            Experience {
                points: experience.points,
                level,
                base_rank: experience.base_rank,
                max_level: experience.max_level,
            },
        )?;
    }
    if let Some(paid_cost) = paid_cost {
        tick.state_mut().insert(
            ids.object_value,
            replacement,
            crate::economy::ObjectValue {
                paid_cost,
                upgrade_spend: 0,
            },
        )?;
    }
    if let Some(mark) = selected {
        tick.state_mut().insert(ids.selected, replacement, mark)?;
    }

    tick.state_mut().despawn(entity);
    Ok(Some(replacement))
}
