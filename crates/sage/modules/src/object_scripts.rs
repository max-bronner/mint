use std::fmt;

use mint_sage_lua::{Budget, Chunk, Function, Host, HostError, ParseError, Value};
use mint_sim::Entity;

use crate::effects::{Effect, Name, Names};
use crate::script_events::{ScriptEvents, ScriptEventsError};

const BOUND: [&str; 13] = [
    "ObjectSetGeometryActive",
    "ObjectHideSubObjectPermanently",
    "ObjectPlayerSide",
    "ObjectCapturingObjectPlayerSide",
    "ObjectGrantUpgrade",
    "ObjectEnterRampageState",
    "ObjectBroadcastEventToEnemies",
    "ObjectBroadcastEventToAllies",
    "ObjectBroadcastEventToCivilians",
    "ObjectBroadcastEventToUnits",
    "ObjectTestModelCondition",
    "ObjectPlaySound",
    "ObjectHideSubObject",
];

const MAX_WALK_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptLoadError {
    Events(ScriptEventsError),
    Script(ParseError),
}

impl fmt::Display for ScriptLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptLoadError::Events(error) => write!(f, "scriptevents: {error}"),
            ScriptLoadError::Script(error) => write!(f, "scripts: {error}"),
        }
    }
}

impl std::error::Error for ScriptLoadError {}

#[derive(Debug, Clone)]
pub struct ObjectScripts {
    chunk: Chunk,
    events: ScriptEvents,
}

impl ObjectScripts {
    pub fn load(events: &str, script: &str) -> Result<Self, ScriptLoadError> {
        Ok(Self {
            chunk: Chunk::parse(script).map_err(ScriptLoadError::Script)?,
            events: ScriptEvents::parse(events).map_err(ScriptLoadError::Events)?,
        })
    }

    #[must_use]
    pub fn run(
        &self,
        list: &str,
        event: &str,
        data: Option<&str>,
        facts: &ObjectFacts<'_>,
        source: EventSource,
        broadcaster: Option<&mut dyn Broadcaster>,
    ) -> Option<HandlerRun> {
        let function = self.events.handler(list, event)?;
        let this = source
            .this
            .map_or(Value::Nil, |entity| Value::Handle(encode_entity(entity)));
        let second = match source.other {
            Some(other) => Value::Handle(encode_entity(other)),
            None => data.map_or(Value::Nil, Value::string),
        };
        let supplied = [this, second];
        let arity = self
            .chunk
            .function(function)
            .map_or(supplied.len(), Function::arity);
        let args: Vec<Value> = (0..arity)
            .map(|at| supplied.get(at).cloned().unwrap_or(Value::Nil))
            .collect();
        let mut host = EventHost {
            facts,
            effects: Vec::new(),
            refused: Vec::new(),
            broadcaster,
        };
        let mut budget = Budget::default();
        let error = self
            .chunk
            .call(function, &args, &mut host, &mut budget)
            .err()
            .map(|error| format!("{function}: {error}"));
        Some(HandlerRun {
            function: function.to_owned(),
            effects: host.effects,
            refused: host.refused,
            error,
        })
    }

    #[must_use]
    pub fn describe(&self) -> String {
        let unbound = self.unbound_calls();
        let calls: Vec<String> = unbound
            .iter()
            .map(|(name, count)| format!("{name} x{count}"))
            .collect();
        format!(
            "scripts: {} list(s), {} handler(s), {} undeclared event name(s), {} missing \
             function(s); {} call(s) not bound in the simulation: {}",
            self.events.lists().count(),
            self.events
                .lists()
                .map(|list| list.handlers.len())
                .sum::<usize>(),
            self.events.unknown_events().len(),
            self.missing_functions().len(),
            unbound.len(),
            calls.join(", ")
        )
    }

    #[must_use]
    pub fn unbound_calls(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for list in self.events.lists() {
            for (_, function) in &list.handlers {
                for name in self.calls_of(function) {
                    match counts.iter_mut().find(|(held, _)| *held == name) {
                        Some((_, count)) => *count = count.saturating_add(1),
                        None => counts.push((name, 1)),
                    }
                }
            }
        }
        counts.sort_by(|(left_name, left), (right_name, right)| {
            right.cmp(left).then_with(|| left_name.cmp(right_name))
        });
        counts
    }

    #[must_use]
    pub fn missing_functions(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .events
            .lists()
            .flat_map(|list| list.handlers.iter())
            .map(|(_, function)| function.as_str())
            .filter(|function| self.chunk.function(function).is_none())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn calls_of(&self, function: &str) -> Vec<String> {
        let mut seen: Vec<&str> = Vec::new();
        let mut pending: Vec<(&str, usize)> = Vec::new();
        let mut out: Vec<String> = Vec::new();
        if let Some(declared) = self.chunk.function(function) {
            pending.push((declared.name(), 0));
        }
        while let Some((name, depth)) = pending.pop() {
            if depth > MAX_WALK_DEPTH || seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let Some(declared) = self.chunk.function(name) else {
                continue;
            };
            for call in declared.call_names() {
                if self.chunk.function(call).is_some() {
                    pending.push((call, depth.saturating_add(1)));
                } else if !BOUND.contains(&call)
                    && !mint_sage_lua::BASE_LIBRARY.contains(&call)
                    && !out.iter().any(|held| held == call)
                {
                    out.push(call.to_owned());
                }
            }
        }
        out.sort();
        out
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectFacts<'a> {
    pub side: Option<&'a str>,
    pub capturing_side: Option<&'a str>,
    pub conditions: &'a [String],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EventSource {
    pub this: Option<Entity>,
    pub other: Option<Entity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastKind {
    Enemies,
    Allies,
    Civilians,
    Units,
}

pub trait Broadcaster {
    fn broadcast(&mut self, kind: BroadcastKind, event: &str, radius: f32);
}

fn encode_entity(entity: Entity) -> u64 {
    (u64::from(entity.index()) << 32) | u64::from(entity.generation())
}

fn to_number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(number) => Some(*number),
        Value::Str(text) => text.trim().parse().ok(),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerRun {
    pub function: String,
    pub effects: Vec<Effect>,
    pub refused: Vec<String>,
    pub error: Option<String>,
}

struct EventHost<'a, 'b> {
    facts: &'a ObjectFacts<'a>,
    effects: Vec<Effect>,
    refused: Vec<String>,
    broadcaster: Option<&'b mut dyn Broadcaster>,
}

impl EventHost<'_, '_> {
    fn sub_object(&mut self, args: &[Value], permanent: bool) -> Result<Value, HostError> {
        let Some(name) = args.get(1).and_then(Value::as_str) else {
            return Err(HostError::new("a sub-object name is needed"));
        };
        let on = match args.get(2) {
            Some(Value::Bool(on)) => *on,
            None => true,
            Some(other) => {
                return Err(HostError::new(format!(
                    "a boolean is needed, not a {}",
                    other.type_name()
                )));
            }
        };
        let name: Names = [Name::from(name)].into();
        let empty: Names = [].into();
        let (show, hide) = if on { (empty, name) } else { (name, empty) };
        self.effects.push(Effect::SubObjects {
            show,
            hide,
            permanent,
        });
        Ok(Value::Nil)
    }

    fn broadcast_call(
        &mut self,
        kind: BroadcastKind,
        name: &str,
        args: &[Value],
    ) -> Result<Value, HostError> {
        let Some(event) = args.get(1).and_then(Value::as_str) else {
            return Err(HostError::new("an event name is needed"));
        };
        let Some(radius) = args.get(2).and_then(to_number) else {
            return Err(HostError::new("a radius is needed"));
        };
        match &mut self.broadcaster {
            Some(broadcaster) => {
                broadcaster.broadcast(kind, event, radius);
                Ok(Value::Nil)
            }
            None => {
                self.refused.push(name.to_owned());
                Err(HostError::new("no world to broadcast into"))
            }
        }
    }
}

impl Host for EventHost<'_, '_> {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError> {
        match name {
            "ObjectHideSubObjectPermanently" => self.sub_object(args, true),
            "ObjectHideSubObject" => self.sub_object(args, false),
            "ObjectSetGeometryActive" => {
                let (Some(name), Some(Value::Bool(active))) =
                    (args.get(1).and_then(Value::as_str), args.get(2))
                else {
                    return Err(HostError::new("a geometry name and a boolean are needed"));
                };
                let name: Names = [Name::from(name)].into();
                let empty: Names = [].into();
                let (show, hide) = if *active {
                    (name, empty)
                } else {
                    (empty, name)
                };
                self.effects.push(Effect::Geometry { show, hide });
                Ok(Value::Nil)
            }
            "ObjectPlayerSide" => Ok(side(self.facts.side)),
            "ObjectCapturingObjectPlayerSide" => Ok(side(self.facts.capturing_side)),
            "ObjectGrantUpgrade" => {
                let Some(upgrade) = args.get(1).and_then(Value::as_str) else {
                    return Err(HostError::new("an upgrade name is needed"));
                };
                self.effects.push(Effect::GrantUpgrade(Name::from(upgrade)));
                Ok(Value::Nil)
            }
            "ObjectEnterRampageState" => {
                self.effects.push(Effect::EnterRampage);
                Ok(Value::Nil)
            }
            "ObjectBroadcastEventToEnemies" => {
                self.broadcast_call(BroadcastKind::Enemies, name, args)
            }
            "ObjectBroadcastEventToAllies" => {
                self.broadcast_call(BroadcastKind::Allies, name, args)
            }
            "ObjectBroadcastEventToCivilians" => {
                self.broadcast_call(BroadcastKind::Civilians, name, args)
            }
            "ObjectBroadcastEventToUnits" => self.broadcast_call(BroadcastKind::Units, name, args),
            "ObjectTestModelCondition" => {
                let Some(condition) = args.get(1).and_then(Value::as_str) else {
                    return Err(HostError::new("a condition name is needed"));
                };
                Ok(Value::Bool(
                    self.facts
                        .conditions
                        .iter()
                        .any(|held| held.eq_ignore_ascii_case(condition)),
                ))
            }
            "ObjectPlaySound" => {
                let Some(event) = args.get(1).and_then(Value::as_str) else {
                    return Err(HostError::new("a sound event name is needed"));
                };
                self.effects
                    .push(Effect::PlayObjectSound(Name::from(event)));
                Ok(Value::Nil)
            }
            other => {
                self.refused.push(other.to_owned());
                Err(HostError::new("not bound in the simulation yet (B41)"))
            }
        }
    }
}

fn side(of: Option<&str>) -> Value {
    of.map_or(Value::Nil, Value::string)
}

#[cfg(test)]
#[path = "object_scripts/tests.rs"]
mod tests;
