use std::sync::Arc;

use mint_sage_ini::EmotionType;
use mint_sage_map::Parameter;
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick};

use super::object_status::Target;
use super::orders::ScriptOrder;
use super::{
    parameter_int, parameter_real, parameter_string, ActionVerb, ForcedModelCondition,
    MapScriptEngine, MapScriptState, ParseCtx, TICKS_PER_SCRIPT_FRAME,
};
use crate::combat::DeathNaming;
use crate::rules::{Ids, RotwkRules};

const EMOTIONS: [&str; 12] = [
    "TAUNT",
    "CHEER",
    "HERO_CHEER",
    "POINT",
    "FEAR",
    "UNCONTROLLABLE_FEAR",
    "TERROR",
    "DOOM",
    "QUARRELSOME",
    "ALERT",
    "BRACE_FOR_BEING_CRUSHED",
    "CHEER_FOR_ABOUT_TO_CRUSH",
];

const USER_CONDITIONS: [&str; 4] = ["USER_1", "USER_2", "USER_3", "USER_4"];

fn emotion_at(index: i32) -> Option<EmotionType> {
    EMOTIONS
        .get(usize::try_from(index).ok()?)
        .and_then(|name| EmotionType::parse(name))
}

fn horde_kill_count(members: usize, percent: f32) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let wanted = members as f32 * percent * 0.01;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let wanted = wanted as usize;
    wanted
}

enum Who {
    Object(Target),
    Players(String),
}

enum Do {
    Emotion {
        kind: Option<EmotionType>,
        lasts: LogicFrames,
    },
    UserCondition {
        name: &'static str,
        on: bool,
    },
    TimedCondition {
        name: String,
        frames: u32,
    },
    Nothing,
    KillHorde {
        percent: f32,
    },
    Shock {
        angle: f32,
        force: f32,
        lift: f32,
    },
}

struct ActionEffect {
    who: Who,
    what: Do,
}

impl RotwkRules {
    fn script_kill_horde(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        percent: f32,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(members) = tick
            .state()
            .get(ids.horde, horde)
            .map(|held| held.members.clone())
        else {
            return Ok(());
        };
        let wanted = horde_kill_count(members.len(), percent);
        if wanted == 0 || wanted > members.len() {
            return Ok(());
        }
        let doomed: Vec<Entity> = members
            .into_iter()
            .filter(|&member| tick.state().contains(ids.health, member))
            .take(wanted)
            .collect();
        for member in doomed {
            if let Some(pair) = self.kill(tick, ids, member, None, DeathNaming::Normal)? {
                kills.push(pair);
            }
        }
        Ok(())
    }
}

impl ActionVerb for ActionEffect {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        current: Option<PlayerId>,
        _: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let subjects = match &self.who {
            Who::Object(target) => target.entities(rules, tick, ids, state),
            Who::Players(text) => rules
                .player_mask(&state.rt, text, current)
                .players()
                .flat_map(|player| RotwkRules::owned_by(tick.state(), ids, player))
                .collect(),
        };
        for entity in subjects {
            match &self.what {
                Do::Emotion { kind, lasts } => {
                    if let Some(kind) = kind {
                        rules.force_emotion(tick, ids, entity, *kind, *lasts, None)?;
                    }
                }
                Do::Nothing => {}
                Do::UserCondition { name, on } => {
                    let held = tick
                        .state()
                        .get(ids.forced_model_condition, entity)
                        .is_some_and(|forced| forced.name == *name);
                    if *on {
                        tick.state_mut().insert(
                            ids.forced_model_condition,
                            entity,
                            ForcedModelCondition {
                                name: (*name).to_owned(),
                                until: None,
                            },
                        )?;
                    } else if held {
                        tick.state_mut().remove(ids.forced_model_condition, entity);
                    }
                }
                Do::TimedCondition { name, frames } => {
                    let ticks = u64::from(*frames).saturating_mul(TICKS_PER_SCRIPT_FRAME);
                    let until = Frame::new(tick.frame().get().saturating_add(ticks));
                    tick.state_mut().insert(
                        ids.forced_model_condition,
                        entity,
                        ForcedModelCondition {
                            name: name.clone(),
                            until: Some(until),
                        },
                    )?;
                }
                Do::KillHorde { percent } => {
                    rules.script_kill_horde(tick, ids, entity, *percent, kills)?;
                }
                Do::Shock { angle, force, lift } => {
                    if rules.shock_admits(tick, ids, entity) {
                        rules.issue_script_order(tick, ids, &[entity], ScriptOrder::Stop)?;
                        rules.script_shock(tick, ids, entity, *angle, *force, *lift)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.who {
            Who::Object(target) => target.unit_name(),
            Who::Players(_) => None,
        }
    }
}

fn effect(who: Option<Who>, what: Option<Do>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionEffect {
        who: who?,
        what: what?,
    }))
}

fn emotion(params: &[Parameter]) -> Option<Do> {
    Some(Do::Emotion {
        kind: emotion_at(parameter_int(params, 1)?),
        lasts: LogicFrames::from_seconds(parameter_real(params, 2)?),
    })
}

pub(super) fn parse_unit_force_emotion(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    effect(
        Target::parse(params, ctx, false).map(Who::Object),
        emotion(params),
    )
}

pub(super) fn parse_team_force_emotion(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    effect(
        Target::parse(params, ctx, true).map(Who::Object),
        emotion(params),
    )
}

pub(super) fn parse_player_force_emotion(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    effect(
        parameter_string(params, 0).map(Who::Players),
        emotion(params),
    )
}

pub(super) fn parse_unit_set_model_condition(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let user = usize::try_from(parameter_int(params, 1)?.saturating_sub(1)).unwrap_or(0);
    let name = USER_CONDITIONS
        .get(user)
        .copied()
        .unwrap_or(USER_CONDITIONS[0]);
    effect(
        Target::parse(params, ctx, false).map(Who::Object),
        Some(Do::UserCondition {
            name,
            on: parameter_int(params, 2)? != 0,
        }),
    )
}

pub(super) fn parse_team_model_condition_for_duration(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let name = parameter_string(params, 1)?;
    let known = mint_sage_ini::model_condition_flags::model_condition_index(&name).is_some();
    let seconds = parameter_real(params, 2)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds * crate::game_data::RETAIL_LOGIC_HZ as f32).max(0.0) as u32;
    let what = if known && frames > 0 {
        Do::TimedCondition { name, frames }
    } else {
        Do::Nothing
    };
    effect(
        Target::parse(params, ctx, true).map(Who::Object),
        Some(what),
    )
}

pub(super) fn parse_kill_horde_members(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    effect(
        Target::parse(params, ctx, false).map(Who::Object),
        Some(Do::KillHorde {
            percent: parameter_real(params, 1)?,
        }),
    )
}

pub(super) fn parse_named_shock(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    effect(
        Target::parse(params, ctx, false).map(Who::Object),
        Some(Do::Shock {
            angle: parameter_real(params, 1)?,
            force: parameter_real(params, 2)?,
            lift: parameter_real(params, 3)?,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_of_five_members_is_two_and_the_emotion_table_ends_at_twelve() {
        assert_eq!(horde_kill_count(5, 50.0), 2);
        assert_eq!(horde_kill_count(5, 100.0), 5);
        assert!(emotion_at(11).is_some());
        assert!(emotion_at(12).is_none());
        assert!(emotion_at(-1).is_none());
    }
}
