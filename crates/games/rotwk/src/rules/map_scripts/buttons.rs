use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_ini::{CommandButtonDef, Stance};
use mint_sage_map::Parameter;
use mint_sim::{CommandKey, Entity, PlayerId, Result, Rules, Tick};

use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkCommand, RotwkRules};

#[derive(Debug, Clone)]
enum Who {
    Named(String),
    Team(String),
}

#[derive(Debug, Clone)]
enum Aim {
    Here,
    Waypoint(Option<Vec2>),
    Named(String),
    NearestEnemy,
    NearestKindOf(String),
    NearestType(String),
}

struct ActionButton {
    who: Who,
    button: String,
    side: String,
    aim: Aim,
    own_set: bool,
    group_aim: bool,
    verb: &'static str,
}

enum Target {
    None,
    At(Vec2),
    On(Entity),
}

impl RotwkRules {
    fn unit_command_set<'a>(
        &'a self,
        tick: &'a Tick<'_>,
        ids: Ids,
        unit: Entity,
    ) -> Option<&'a str> {
        self.command_set_override(tick.state(), unit).or_else(|| {
            let kind = tick.state().get(ids.kind, unit)?;
            self.kinds.get(*kind)?.record.command_set.as_deref()
        })
    }

    fn slots_holding(&self, tick: &Tick<'_>, ids: Ids, unit: Entity, button: &str) -> usize {
        let Some(set) = self
            .unit_command_set(tick, ids, unit)
            .and_then(|name| self.command_sets.get(name))
        else {
            return 0;
        };
        set.slots
            .values()
            .filter(|held| held.eq_ignore_ascii_case(button))
            .count()
    }

    pub(super) fn button_command(def: &CommandButtonDef) -> String {
        def.command
            .as_deref()
            .map(str::to_ascii_uppercase)
            .unwrap_or_default()
    }

    pub(super) fn unit_has_power(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
        power: &str,
    ) -> bool {
        tick.state()
            .get(ids.kind, unit)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.power(power).is_some())
    }

    fn button_ready(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
        def: &CommandButtonDef,
    ) -> bool {
        match Self::button_command(def).as_str() {
            "SPECIAL_POWER" | "SPECIAL_POWER_TOGGLE" | "SPELL_BOOK" => {
                def.special_power.as_deref().is_some_and(|power| {
                    self.unit_has_power(tick, ids, unit, power)
                        && self.power_is_ready(tick.state(), ids, tick.frame(), unit, power)
                })
            }
            _ => true,
        }
    }

    fn upgrade_index(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
        name: &str,
        player_kind: bool,
    ) -> Option<u8> {
        let set = self
            .unit_command_set(tick, ids, unit)
            .and_then(|set| self.command_sets.get(set))?;
        let mut index = 0u8;
        for held in set.slots.values() {
            let Some(def) = self.ai_power_buttons.get(held) else {
                continue;
            };
            let command = Self::button_command(def);
            let counts = def.upgrade.is_some()
                && if player_kind {
                    command == "PLAYER_UPGRADE"
                } else {
                    command == "OBJECT_UPGRADE" || command == "CASTLE_UPGRADE"
                };
            if !counts {
                continue;
            }
            if held.eq_ignore_ascii_case(name) {
                return Some(index);
            }
            index = index.saturating_add(1);
        }
        None
    }

    fn press_button(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        name: &str,
        def: &CommandButtonDef,
        target: &Target,
    ) -> Result<bool> {
        let Some(owner) = tick.state().get(ids.owner, unit).map(|owner| owner.0) else {
            return Ok(false);
        };
        let command = Self::button_command(def);
        match command.as_str() {
            "SPECIAL_POWER" | "SPECIAL_POWER_TOGGLE" | "SPELL_BOOK" => {
                let Some(power) = def.special_power.as_deref() else {
                    return Ok(false);
                };
                if !self.unit_has_power(tick, ids, unit, power) {
                    return Ok(false);
                }
                let (object, at) = match target {
                    Target::None => (None, None),
                    Target::At(point) => (None, Some((point.x, point.y))),
                    Target::On(entity) => (Some(*entity), None),
                };
                self.fire_power_scripted(tick, ids, unit, power, object, at)?;
                Ok(true)
            }
            "TOGGLE_WEAPONSET" => {
                let button = name.to_owned();
                self.as_owner_selection(tick, ids, &[unit], |rules, tick, actor| {
                    rules.apply(
                        tick,
                        CommandKey::new(tick.frame(), actor, 0),
                        &RotwkCommand::ToggleWeaponSet {
                            button: button.clone(),
                            lead: None,
                        },
                    )
                })?;
                Ok(true)
            }
            "OBJECT_UPGRADE" | "CASTLE_UPGRADE" | "PLAYER_UPGRADE" => {
                let player_kind = command == "PLAYER_UPGRADE";
                let Some(index) = self.upgrade_index(tick, ids, unit, name, player_kind) else {
                    return Ok(false);
                };
                let order = if player_kind {
                    RotwkCommand::PlayerUpgrade { button: index }
                } else {
                    RotwkCommand::ObjectUpgrade { button: index }
                };
                self.as_owner_selection(tick, ids, &[unit], |rules, tick, actor| {
                    rules.apply(tick, CommandKey::new(tick.frame(), actor, 0), &order)
                })?;
                Ok(true)
            }
            "SET_STANCE" | "TOGGLE_STANCE" => {
                let listed: Vec<Stance> = def
                    .stances
                    .iter()
                    .filter_map(|stance| Stance::from_name(stance))
                    .collect();
                let current = tick.state().get(ids.stance, unit).map(|held| held.0);
                let next = if command == "SET_STANCE" {
                    listed.first().copied()
                } else {
                    let at = current.and_then(|now| listed.iter().position(|s| *s == now));
                    let step = at.map_or(0, |at| at.saturating_add(1));
                    listed.get(step).or_else(|| listed.first()).copied()
                };
                let Some(next) = next else {
                    return Ok(false);
                };
                self.as_owner_selection(tick, ids, &[unit], |rules, tick, actor| {
                    rules.apply(
                        tick,
                        CommandKey::new(tick.frame(), actor, 0),
                        &RotwkCommand::SetStance {
                            stance: next.index(),
                        },
                    )
                })?;
                Ok(true)
            }
            "FIRE_WEAPON" => {
                let (Target::At(point), Some(slot)) = (target, def.weapon_slot) else {
                    diag!("script: {name} is a weapon button with no position - skipped");
                    return Ok(false);
                };
                let point = *point;
                self.as_owner_selection(tick, ids, &[unit], |rules, tick, actor| {
                    rules.apply(
                        tick,
                        CommandKey::new(tick.frame(), actor, 0),
                        &RotwkCommand::AttackGround {
                            slot,
                            x: point.x,
                            y: point.y,
                        },
                    )
                })?;
                Ok(true)
            }
            _ => {
                diag!("script: command button {name} ({command}) is not run by scripts here");
                let _ = owner;
                Ok(false)
            }
        }
    }

    fn nearest_enemy_to(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        scanner: Entity,
        from: Vec2,
    ) -> Option<Entity> {
        self.hostile_foes(tick, ids, scanner)
            .into_iter()
            .map(|foe| {
                let (dx, dy) = (foe.position.x - from.x, foe.position.y - from.y);
                (dx * dx + dy * dy, foe.entity)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index().cmp(&b.1.index())))
            .map(|(_, entity)| entity)
    }
}

impl ActionButton {
    fn members(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
    ) -> Vec<Entity> {
        match &self.who {
            Who::Named(name) => rules
                .object_named(tick, ids, &state.rt, name, &self.side)
                .into_iter()
                .collect(),
            Who::Team(team) => rules.subject_members(tick, ids, state, team, Some(&self.side)),
        }
    }

    fn target(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        scanner: Entity,
        centre: Vec2,
        def: &CommandButtonDef,
    ) -> Option<Target> {
        let at_position = def
            .options
            .iter()
            .any(|o| o.eq_ignore_ascii_case("NEED_TARGET_POS"));
        let aim_at = |found: Entity| {
            if at_position {
                rules.position_of_entity(tick, ids, found).map(Target::At)
            } else {
                Some(Target::On(found))
            }
        };
        match &self.aim {
            Aim::Here => Some(Target::None),
            Aim::Waypoint(point) => point.map(Target::At),
            Aim::Named(name) => {
                let found = rules.object_named(tick, ids, &state.rt, name, &self.side)?;
                aim_at(found)
            }
            Aim::NearestEnemy => aim_at(rules.nearest_enemy_to(tick, ids, scanner, centre)?),
            Aim::NearestKindOf(kind) => {
                let found = rules.nearest_matching(tick, ids, centre, |entity| {
                    entity != scanner && rules.has_kind_bit(tick.state(), ids, entity, kind)
                })?;
                aim_at(found)
            }
            Aim::NearestType(types) => {
                let names = state.rt.lists.types_of(types);
                let found = rules.nearest_of_types(tick, ids, &names, centre, None, false)?;
                aim_at(found)
            }
        }
    }
}

impl RotwkRules {
    fn nearest_matching(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        from: Vec2,
        keep: impl Fn(Entity) -> bool,
    ) -> Option<Entity> {
        let world = tick.state();
        world
            .store(ids.kind)?
            .iter()
            .map(|(entity, _)| entity)
            .filter(|&entity| {
                world.is_live(entity) && !world.contains(ids.doomed, entity) && keep(entity)
            })
            .filter_map(|entity| {
                let at = self.position_of_entity(tick, ids, entity)?;
                let (dx, dy) = (at.x - from.x, at.y - from.y);
                Some((dx * dx + dy * dy, entity))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index().cmp(&b.1.index())))
            .map(|(_, entity)| entity)
    }
}

impl ActionVerb for ActionButton {
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
        let Some(def) = rules.ai_power_buttons.get(&self.button).cloned() else {
            diag!(
                "script: {} names unknown button {:?}",
                self.verb,
                self.button
            );
            return Ok(());
        };
        let members = self.members(rules, tick, ids, state);
        let nearest_form = matches!(
            self.aim,
            Aim::NearestEnemy | Aim::NearestKindOf(_) | Aim::NearestType(_)
        );
        let positions: Vec<Vec2> = members
            .iter()
            .filter_map(|&m| rules.position_of_entity(tick, ids, m))
            .collect();
        let group_centre = super::cinematic::mean_position(&positions);
        let mut pressed = 0usize;
        for &unit in &members {
            let presses = if self.own_set {
                rules.slots_holding(tick, ids, unit, &self.button)
            } else {
                1
            };
            if presses == 0 {
                continue;
            }
            if nearest_form && !rules.button_ready(tick, ids, unit, &def) {
                continue;
            }
            let centre = if self.group_aim {
                group_centre
            } else {
                rules.position_of_entity(tick, ids, unit)
            };
            let Some(centre) = centre.or((!nearest_form).then_some(Vec2::ZERO)) else {
                continue;
            };
            let Some(target) = self.target(rules, tick, ids, state, unit, centre, &def) else {
                continue;
            };
            for _ in 0..presses {
                if rules.press_button(tick, ids, unit, &self.button, &def, &target)? {
                    pressed = pressed.saturating_add(1);
                }
            }
        }
        diag!(
            "script: {} {:?} pressed {pressed} time(s) for {} unit(s)",
            self.verb,
            self.button,
            members.len()
        );
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.who {
            Who::Named(name) => Some(name),
            Who::Team(_) => None,
        }
    }
}

struct ActionAutoAbility {
    who: Who,
    button: String,
    side: String,
    on: bool,
}

impl ActionVerb for ActionAutoAbility {
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
        let Some(power) = rules
            .ai_power_buttons
            .get(&self.button)
            .filter(|def| def.auto_ability == Some(true))
            .and_then(|def| def.special_power.clone())
        else {
            return Ok(());
        };
        let members = match &self.who {
            Who::Named(name) => rules
                .object_named(tick, ids, &state.rt, name, &self.side)
                .into_iter()
                .collect(),
            Who::Team(team) => rules.subject_members(tick, ids, state, team, Some(&self.side)),
        };
        for unit in members {
            if rules.slots_holding(tick, ids, unit, &self.button) == 0 {
                continue;
            }
            if rules.autocast_enabled(tick, ids, unit, &power) != self.on {
                rules.toggle_auto_cast(tick, ids, unit, &power)?;
            }
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.who {
            Who::Named(name) => Some(name),
            Who::Team(_) => None,
        }
    }
}

fn button(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    aim: impl FnOnce(&[Parameter], &ParseCtx<'_>) -> Option<Aim>,
    own_set: bool,
    group_aim: bool,
    verb: &'static str,
) -> Option<Arc<dyn ActionVerb>> {
    let subject = parameter_string(params, 0)?;
    Some(Arc::new(ActionButton {
        who: if named {
            Who::Named(subject)
        } else {
            Who::Team(subject)
        },
        button: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
        aim: aim(params, ctx)?,
        own_set,
        group_aim,
        verb,
    }))
}

fn waypoint_aim(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Aim> {
    let name = parameter_string(params, 2)?;
    Some(Aim::Waypoint(
        ctx.waypoints.get(&name).map(|(point, _)| *point),
    ))
}

macro_rules! rows {
    ($($fn_name:ident, $named:expr, $own:expr, $group:expr, $verb:literal, $aim:expr;)*) => {
        $(pub(super) fn $fn_name(
            params: &[Parameter],
            ctx: &ParseCtx<'_>,
        ) -> Option<Arc<dyn ActionVerb>> {
            button(params, ctx, $named, $aim, $own, $group, $verb)
        })*
    };
}

rows! {
    parse_named_ability, true, true, false, "NAMED_USE_COMMANDBUTTON_ABILITY", |_, _| Some(Aim::Here);
    parse_named_ability_at_waypoint, true, true, false,
        "NAMED_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT", waypoint_aim;
    parse_named_ability_on_named, true, true, false, "NAMED_USE_COMMANDBUTTON_ABILITY_ON_NAMED",
        |p, _| parameter_string(p, 2).map(Aim::Named);
    parse_team_ability, false, false, false, "TEAM_USE_COMMANDBUTTON_ABILITY", |_, _| Some(Aim::Here);
    parse_team_ability_at_waypoint, false, false, false,
        "TEAM_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT", waypoint_aim;
    parse_named_nearest_enemy, true, false, false, "NAMED_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT",
        |_, _| Some(Aim::NearestEnemy);
    parse_named_nearest_kindof, true, false, false, "NAMED_USE_COMMANDBUTTON_ON_NEAREST_KINDOF",
        |p, _| parameter_string(p, 2).map(Aim::NearestKindOf);
    parse_named_nearest_type, true, false, false, "NAMED_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE",
        |p, _| parameter_string(p, 2).map(Aim::NearestType);
    parse_team_all_nearest_enemy, false, false, true,
        "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT", |_, _| Some(Aim::NearestEnemy);
}

fn auto(params: &[Parameter], ctx: &ParseCtx<'_>, named: bool) -> Option<Arc<dyn ActionVerb>> {
    let subject = parameter_string(params, 0)?;
    Some(Arc::new(ActionAutoAbility {
        who: if named {
            Who::Named(subject)
        } else {
            Who::Team(subject)
        },
        button: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
        on: parameter_int(params, 2)? != 0,
    }))
}

pub(super) fn parse_named_auto(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    auto(params, ctx, true)
}

pub(super) fn parse_team_auto(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    auto(params, ctx, false)
}
