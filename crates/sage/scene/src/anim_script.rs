use std::collections::BTreeMap;

use mint_sage_ini::AnimationStateDef;
use mint_sage_lua::{Budget, Env, Host, HostError, ParseError, Script, Value};

use crate::animation_state::{variant_fraction, Conditions, ExtraTokens};

pub fn compile_states(states: &[AnimationStateDef]) -> (Vec<Option<Script>>, ScriptLoadReport) {
    let mut entries = Vec::with_capacity(states.len());
    let mut report = ScriptLoadReport::default();
    for state in states {
        let Some(raw) = &state.script else {
            entries.push(None);
            continue;
        };
        match Script::parse(&raw.body) {
            Ok(script) => {
                report.parsed = report.parsed.saturating_add(1);
                entries.push(Some(script));
            }
            Err(error) => {
                report.failed = report.failed.saturating_add(1);
                if report.failures.len() < MAX_REPORTED_FAILURES {
                    report
                        .failures
                        .push(format_failure(&raw.file, raw.line, &error));
                }
                entries.push(None);
            }
        }
    }
    (entries, report)
}

const MAX_REPORTED_FAILURES: usize = 64;

fn format_failure(file: &str, line: u32, error: &ParseError) -> String {
    format!("{file}:{line}: {error}")
}

#[derive(Debug, Clone, Default)]
pub struct ScriptLoadReport {
    pub parsed: usize,
    pub failed: usize,
    pub failures: Vec<String>,
}

impl ScriptLoadReport {
    pub fn add(&mut self, other: &Self) {
        self.parsed = self.parsed.saturating_add(other.parsed);
        self.failed = self.failed.saturating_add(other.failed);
        for failure in &other.failures {
            if self.failures.len() >= MAX_REPORTED_FAILURES {
                break;
            }
            self.failures.push(failure.clone());
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refusals {
    counts: BTreeMap<String, u64>,
}

impl Refusals {
    fn count(&mut self, name: &str) {
        let slot = self.counts.entry(name.to_owned()).or_insert(0);
        *slot = slot.saturating_add(1);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.counts
            .iter()
            .map(|(name, count)| (name.as_str(), *count))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn add(&mut self, other: &Self) {
        for (name, count) in &other.counts {
            let slot = self.counts.entry(name.clone()).or_insert(0);
            *slot = slot.saturating_add(*count);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptRun {
    pub transitions: Vec<String>,
    pub label: Option<String>,
    pub sub_objects: Vec<(String, bool, bool)>,
    pub modules: Vec<(String, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prev<'a> {
    pub state: &'a str,
    pub animation: &'a str,
    pub fraction: f32,
}

struct DrawableHost<'a> {
    prev: Prev<'a>,
    conditions: Conditions,
    extras: &'a ExtraTokens,
    id: u32,
    state: u32,
    run: ScriptRun,
    refused: &'a mut Refusals,
}

const SUB_OBJECT_CALLS: [(&str, bool, bool); 4] = [
    ("CurDrawableHideSubObject", false, false),
    ("CurDrawableShowSubObject", true, false),
    ("CurDrawableHideSubObjectPermanently", false, true),
    ("CurDrawableShowSubObjectPermanently", true, true),
];

const BOUND_NOOP_CALLS: [&str; 2] = ["CurDrawableAllowToContinue", "CurDrawablePlaySound"];

const MODULE_CALLS: [(&str, bool); 2] = [
    ("CurDrawableHideModule", false),
    ("CurDrawableShowModule", true),
];

const SCRIPT_RANDOM_SALT: u32 = 0x05;

impl Host for DrawableHost<'_> {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError> {
        if name == "CurDrawablePrevAnimationState" {
            return Ok(Value::string(self.prev.state));
        }
        if name == "CurDrawablePrevAnimation" {
            return Ok(Value::string(self.prev.animation));
        }
        if name == "CurDrawablePrevAnimFraction" {
            return Ok(Value::Number(self.prev.fraction));
        }
        if name == "CurDrawableSetTransitionAnimState" {
            if let Some(name) = args.first().and_then(Value::as_str) {
                self.run.transitions.push(name.to_owned());
            }
            return Ok(Value::Nil);
        }
        if let Some((_, shown, permanent)) =
            SUB_OBJECT_CALLS.iter().find(|(call, ..)| *call == name)
        {
            if let Some(part) = args.first().and_then(Value::as_str).map(str::trim) {
                if !part.is_empty() {
                    self.run
                        .sub_objects
                        .push((part.to_owned(), *shown, *permanent));
                }
            }
            return Ok(Value::Nil);
        }
        if let Some((_, shown)) = MODULE_CALLS.iter().find(|(call, _)| *call == name) {
            if let Some(tag) = args.first().and_then(Value::as_str).map(str::trim) {
                if !tag.is_empty() {
                    self.run.modules.push((tag.to_owned(), *shown));
                }
            }
            return Ok(Value::Nil);
        }
        if name == "CurDrawableModelcondition" {
            let live = args
                .first()
                .and_then(Value::as_str)
                .and_then(|token| {
                    Conditions::from_token(token).or_else(|| self.extras.lookup(token))
                })
                .is_some_and(|bit| self.conditions.contains(bit));
            return Ok(Value::Bool(live));
        }
        if name == "GetClientRandomNumberReal" {
            let min = args.first().and_then(Value::as_number).unwrap_or(0.0);
            let max = args.get(1).and_then(Value::as_number).unwrap_or(1.0);
            let frac = variant_fraction(self.id, self.state, SCRIPT_RANDOM_SALT);
            return Ok(Value::Number(min + (max - min) * frac));
        }
        if BOUND_NOOP_CALLS.contains(&name) {
            return Ok(Value::Nil);
        }
        self.refused.count(name);
        Err(HostError::new(format!("`{name}` is not bound")))
    }
}

pub fn run(
    script: &Script,
    prev: Prev<'_>,
    conditions: Conditions,
    extras: &ExtraTokens,
    id: u32,
    state: u32,
    env: &mut Env,
    refused: &mut Refusals,
) -> ScriptRun {
    env.clear();
    let mut host = DrawableHost {
        prev,
        conditions,
        extras,
        id,
        state,
        run: ScriptRun::default(),
        refused,
    };
    let mut budget = Budget::default();
    let outcome = script.run(env, &mut host, &mut budget);
    let mut run = host.run;
    if let Ok(Value::Str(label)) = outcome {
        run.label = Some(label.to_string());
    }
    run
}

#[cfg(test)]
mod tests;
