use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{CommandKey, Entity, PlayerId, Result, Rules, Tick};

use super::{
    parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkCommand, RotwkRules};

struct BuildAtWaypoint {
    builder: String,
    template: String,
    angle: f32,
    at: Option<mint_movement::Vec2>,
    side: String,
}

impl ActionVerb for BuildAtWaypoint {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let (Some(at), Some(kind), Some(builder)) = (
            self.at,
            rules.kinds.kind(&self.template),
            rules.object_named(tick, ids, &state.rt, &self.builder, &self.side),
        ) else {
            return Ok(());
        };
        let angle = self.angle;
        if rules.script_construct(tick, ids, builder, kind, (at, angle))? {
            return Ok(());
        }
        let order = RotwkCommand::Construct {
            kind: kind.0,
            x: at.x,
            y: at.y,
            angle,
        };
        rules.as_owner_selection(tick, ids, &[builder], |rules, tick, actor| {
            rules.apply(tick, CommandKey::new(tick.frame(), actor, 0), &order)
        })?;
        diag!(
            "script: NAMED_BUILD_STRUCTURE_AT_WAYPOINT {:?} orders {:?} at ({:.0}, {:.0})",
            self.builder,
            self.template,
            at.x,
            at.y
        );
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.builder)
    }
}

struct BuildOnFoundation {
    template: String,
    foundation: String,
    side: String,
}

impl RotwkRules {
    fn foundation_button(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        foundation: Entity,
        template: &str,
    ) -> Option<u8> {
        let kind = tick.state().get(ids.kind, foundation)?;
        let set = self
            .command_set_override(tick.state(), foundation)
            .or(self.kinds.get(*kind)?.record.command_set.as_deref())
            .and_then(|name| self.command_sets.get(name))?;
        let mut index = 0u8;
        for held in set.slots.values() {
            let Some(def) = self.ai_power_buttons.get(held) else {
                continue;
            };
            if !def
                .command
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("FOUNDATION_CONSTRUCT"))
            {
                continue;
            }
            if def
                .object
                .as_deref()
                .is_some_and(|object| object.eq_ignore_ascii_case(template))
            {
                return Some(index);
            }
            index = index.saturating_add(1);
        }
        None
    }
}

impl ActionVerb for BuildOnFoundation {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(foundation) =
            rules.object_named(tick, ids, &state.rt, &self.foundation, &self.side)
        else {
            return Ok(());
        };
        let owner = tick.state().get(ids.owner, foundation).map(|owner| owner.0);
        if owner.is_none() || owner != player {
            return Ok(());
        }
        let Some(button) = rules.foundation_button(tick, ids, foundation, &self.template) else {
            diag!(
                "script: BUILD_BUILDING_ON_FOUNDATION: {:?} offers no button building {:?}",
                self.foundation,
                self.template
            );
            return Ok(());
        };
        rules.as_owner_selection(tick, ids, &[foundation], |rules, tick, actor| {
            rules.apply(
                tick,
                CommandKey::new(tick.frame(), actor, 0),
                &RotwkCommand::FoundationConstruct { button },
            )
        })
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.foundation)
    }
}

pub(super) fn parse_build_at_waypoint(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 3)?;
    Some(Arc::new(BuildAtWaypoint {
        builder: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        angle: parameter_real(params, 2)?,
        at: ctx.waypoints.get(&waypoint).map(|(at, _)| *at),
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_build_on_foundation(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(BuildOnFoundation {
        template: parameter_string(params, 0)?,
        foundation: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}
