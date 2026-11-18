use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::orders::ScriptOrder;
use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

impl RotwkRules {
    fn garrison_takes(&self, tick: &Tick<'_>, ids: Ids, container: Entity, unit: Entity) -> bool {
        let world = tick.state();
        let Some(owner) = world.get(ids.owner, unit).map(|held| held.0) else {
            return false;
        };
        self.contain_admits(world, ids, owner, container, unit)
            .is_ok()
    }

    fn garrison_at_once(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        unit: Entity,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, unit).map(|held| held.0) else {
            return Ok(());
        };
        self.absorb(tick, ids, container, unit, owner)?;
        let walkers: Vec<Entity> = Self::object_and_members(tick.state(), ids, unit)
            .into_iter()
            .skip(1)
            .collect();
        self.take_in(tick, ids, container, &walkers)
    }

    fn garrison_occupants_agree(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        container: Entity,
        unit: Entity,
    ) -> bool {
        let world = tick.state();
        let Some(owner) = world.get(ids.owner, unit).map(|held| held.0) else {
            return false;
        };
        let held = world
            .get(ids.passengers, container)
            .map(|list| list.0.clone())
            .unwrap_or_default();
        if held.is_empty() {
            return self.has_kind_bit(world, ids, container, "GARRISON");
        }
        held.iter()
            .all(|&inside| world.get(ids.owner, inside).is_some_and(|o| o.0 == owner))
    }
}

enum Who {
    Named(String),
    Team(String),
}

enum What {
    Garrison { building: String, instantly: bool },
    GarrisonTeam { into: String },
    Evacuate { building: String },
}

struct ActionGarrison {
    who: Who,
    side: String,
    what: What,
    verb: &'static str,
}

impl ActionVerb for ActionGarrison {
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
        if let What::Evacuate { building } = &self.what {
            let Some(container) = rules.object_named(tick, ids, &state.rt, building, &self.side)
            else {
                return Ok(());
            };
            if rules.has_kind_bit(tick.state(), ids, container, "STRUCTURE") {
                rules.schedule_exit(tick, ids, container)?;
                diag!("script: {} empties entity {}", self.verb, container.index());
            }
            return Ok(());
        }
        let mut members = match &self.who {
            Who::Named(name) => rules.subject_members(tick, ids, state, name, None),
            Who::Team(name) => rules.subject_members(tick, ids, state, name, Some(&self.side)),
        };
        members.retain(|&member| !tick.state().contains(ids.member, member));
        let mut taken = 0_usize;
        match &self.what {
            What::Garrison {
                building,
                instantly,
            } => {
                let Some(container) =
                    rules.object_named(tick, ids, &state.rt, building, &self.side)
                else {
                    return Ok(());
                };
                for member in members {
                    if tick.state().contains(ids.contained, member) {
                        continue;
                    }
                    let named = matches!(self.who, Who::Named(_));
                    if named && !rules.garrison_occupants_agree(tick, ids, container, member) {
                        continue;
                    }
                    if !rules.garrison_takes(tick, ids, container, member) {
                        continue;
                    }
                    if *instantly {
                        rules.garrison_at_once(tick, ids, container, member)?;
                    } else {
                        rules.issue_script_order(
                            tick,
                            ids,
                            &[member],
                            ScriptOrder::Enter(container),
                        )?;
                    }
                    taken += 1;
                }
            }
            What::GarrisonTeam { into } => {
                let containers: Vec<Entity> =
                    rules.subject_members(tick, ids, state, into, Some(&self.side));
                for member in members {
                    if tick.state().contains(ids.contained, member) {
                        continue;
                    }
                    let room = containers
                        .iter()
                        .copied()
                        .find(|&c| rules.garrison_takes(tick, ids, c, member));
                    if let Some(container) = room {
                        rules.garrison_at_once(tick, ids, container, member)?;
                        taken += 1;
                    }
                }
            }
            What::Evacuate { .. } => {}
        }
        diag!("script: {} garrisons {taken} unit(s)", self.verb);
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.who {
            Who::Named(name) => Some(name),
            Who::Team(_) => None,
        }
    }
}

fn build(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    verb: &'static str,
    what: impl FnOnce(String) -> What,
) -> Option<Arc<dyn ActionVerb>> {
    let subject = parameter_string(params, 0)?;
    Some(Arc::new(ActionGarrison {
        who: if named {
            Who::Named(subject)
        } else {
            Who::Team(subject)
        },
        side: ctx.side.to_owned(),
        what: what(parameter_string(params, 1)?),
        verb,
    }))
}

macro_rules! garrison_rows {
    ($($fn_name:ident, $named:expr, $verb:literal, $instantly:expr;)*) => {
        $(pub(super) fn $fn_name(
            params: &[Parameter],
            ctx: &ParseCtx<'_>,
        ) -> Option<Arc<dyn ActionVerb>> {
            build(params, ctx, $named, $verb, |building| What::Garrison {
                building,
                instantly: $instantly,
            })
        })*
    };
}

garrison_rows! {
    parse_team_garrison_instantly, false, "TEAM_GARRISON_SPECIFIC_BUILDING_INSTANTLY", true;
    parse_team_garrison, false, "TEAM_GARRISON_SPECIFIC_BUILDING", false;
    parse_named_garrison_instantly, true, "NAMED_GARRISON_SPECIFIC_BUILDING_INSTANTLY", true;
    parse_named_garrison, true, "NAMED_GARRISON_SPECIFIC_BUILDING", false;
}

pub(super) fn parse_team_garrison_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, false, "TEAM_GARRISON_TEAM_INSTANTLY", |into| {
        What::GarrisonTeam { into }
    })
}

pub(super) fn parse_exit_building(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionGarrison {
        who: Who::Team(String::new()),
        side: ctx.side.to_owned(),
        what: What::Evacuate {
            building: parameter_string(params, 0)?,
        },
        verb: "EXIT_SPECIFIC_BUILDING",
    }))
}
