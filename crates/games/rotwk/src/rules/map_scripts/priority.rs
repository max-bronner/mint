use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::combat::priority::{AttackPriorityData, PrioritySet, DEFAULT_SET};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AttackPriorities {
    sets: BTreeMap<String, PrioritySet>,
    unit_sets: BTreeMap<Entity, (String, u64)>,
    team_sets: BTreeMap<usize, String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SetHolder<'a> {
    pub(crate) entity: Entity,
    pub(crate) joined_at: u64,
    pub(crate) team_proto: Option<usize>,
    pub(crate) module_set: Option<&'a str>,
}

impl AttackPriorities {
    fn set_mut(&mut self, name: &str, data: &AttackPriorityData) -> &mut PrioritySet {
        let key = name.to_ascii_lowercase();
        self.sets
            .entry(key.clone())
            .or_insert_with(|| data.sets.get(&key).cloned().unwrap_or_default())
    }

    fn set_priority(
        &mut self,
        set: &str,
        template: &str,
        priority: i32,
        data: &AttackPriorityData,
    ) {
        self.set_mut(set, data)
            .templates
            .insert(template.to_ascii_lowercase(), priority);
    }

    fn resolve(&self, name: &str, data: &AttackPriorityData) -> String {
        let key = name.to_ascii_lowercase();
        if self.sets.contains_key(&key) || data.sets.contains_key(&key) {
            key
        } else {
            String::new()
        }
    }

    fn lookup<'a>(&'a self, key: &str, data: &'a AttackPriorityData) -> &'a PrioritySet {
        self.sets
            .get(key)
            .or_else(|| data.sets.get(key))
            .unwrap_or(&DEFAULT_SET)
    }

    fn apply_to_unit(&mut self, unit: Entity, set: &str, stamp: u64, data: &AttackPriorityData) {
        let key = self.resolve(set, data);
        self.unit_sets.insert(unit, (key, stamp));
    }

    fn apply_to_team(
        &mut self,
        proto: usize,
        members: &[Entity],
        set: &str,
        stamp: u64,
        data: &AttackPriorityData,
    ) {
        let key = self.resolve(set, data);
        for &member in members {
            self.unit_sets.insert(member, (key.clone(), stamp));
        }
        if !key.is_empty() {
            self.team_sets.insert(proto, key);
        }
    }

    pub(crate) fn set_of<'a>(
        &'a self,
        holder: SetHolder<'_>,
        data: &'a AttackPriorityData,
    ) -> Option<&'a PrioritySet> {
        let team_set = holder
            .team_proto
            .and_then(|proto| self.team_sets.get(&proto));
        let explicit = self
            .unit_sets
            .get(&holder.entity)
            .filter(|(_, stamp)| *stamp >= holder.joined_at || team_set.is_none());
        if let Some((key, _)) = explicit {
            return Some(self.lookup(key, data));
        }
        if let Some(key) = team_set {
            return Some(self.lookup(key, data));
        }
        let name = holder.module_set?;
        Some(self.lookup(&self.resolve(name, data), data))
    }

    #[cfg(test)]
    pub(crate) fn templates_of(&self, set: &str) -> Vec<&str> {
        self.sets
            .get(&set.to_ascii_lowercase())
            .map(|set| set.templates.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn name_of(&self, unit: Entity) -> Option<&str> {
        self.unit_sets.get(&unit).map(|(name, _)| name.as_str())
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.unit_sets.retain(|entity, _| is_live(*entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.sets.is_empty() && self.unit_sets.is_empty() && self.team_sets.is_empty() {
            return;
        }
        hasher.write_usize(self.sets.len());
        for (name, set) in &self.sets {
            hasher.write_bytes(name.as_bytes());
            hasher.write_u8(0);
            hasher.write_i32(set.default);
            hasher.write_usize(set.templates.len());
            for (template, priority) in &set.templates {
                hasher.write_bytes(template.as_bytes());
                hasher.write_u8(0);
                hasher.write_i32(*priority);
            }
        }
        hasher.write_usize(self.unit_sets.len());
        for (entity, (name, stamp)) in &self.unit_sets {
            entity.hash_into(hasher);
            hasher.write_bytes(name.as_bytes());
            hasher.write_u8(0);
            hasher.write_u64(*stamp);
        }
        hasher.write_usize(self.team_sets.len());
        for (proto, name) in &self.team_sets {
            hasher.write_usize(*proto);
            hasher.write_bytes(name.as_bytes());
            hasher.write_u8(0);
        }
    }
}

impl RotwkRules {
    pub(crate) fn priority_set_of<'a>(
        &'a self,
        world: &'a World,
        ids: Ids,
        entity: Entity,
    ) -> Option<&'a PrioritySet> {
        let module_set = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.ai.attack_priority.as_deref());
        let rt = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .map(|(_, state)| &state.rt);
        let Some(rt) = rt else {
            let name = module_set?.to_ascii_lowercase();
            return Some(
                self.attack_priorities
                    .sets
                    .get(&name)
                    .unwrap_or(&DEFAULT_SET),
            );
        };
        let holder = SetHolder {
            entity,
            joined_at: rt.teams.joined_at(entity),
            team_proto: rt.teams.proto_of(entity),
            module_set,
        };
        rt.priorities.set_of(holder, &self.attack_priorities)
    }
}

struct ActionObjectListEdit {
    list: String,
    template: String,
    add: bool,
}

impl ActionVerb for ActionObjectListEdit {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if self.add {
            state.rt.lists.add(&self.list, &self.template);
        } else {
            state.rt.lists.remove(&self.list, &self.template);
        }
        Ok(())
    }
}

struct ActionSetPriorityThing {
    set: String,
    thing: String,
    priority: i32,
}

impl ActionVerb for ActionSetPriorityThing {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state
            .rt
            .priorities
            .set_mut(&self.set, &rules.attack_priorities);
        for name in state.rt.lists.types_of(&self.thing) {
            if rules.kinds.kind(&name).is_none()
                && !rules
                    .attack_priorities
                    .defined_templates
                    .contains(&name.to_ascii_lowercase())
            {
                diag!(
                    "script: SET_ATTACK_PRIORITY_THING names unknown template {name:?}; the rest \
                     of {:?} is skipped",
                    self.thing
                );
                break;
            }
            state.rt.priorities.set_priority(
                &self.set,
                &name,
                self.priority,
                &rules.attack_priorities,
            );
        }
        Ok(())
    }
}

struct ActionSetPriorityKindOf {
    set: String,
    kind_of: String,
    priority: i32,
}

impl ActionVerb for ActionSetPriorityKindOf {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state
            .rt
            .priorities
            .set_mut(&self.set, &rules.attack_priorities);
        for entry in rules.kinds.iter() {
            if entry.record.has_kind(&self.kind_of) {
                state.rt.priorities.set_priority(
                    &self.set,
                    &entry.record.name,
                    self.priority,
                    &rules.attack_priorities,
                );
            }
        }
        Ok(())
    }
}

struct ActionSetDefaultPriority {
    set: String,
    priority: i32,
}

impl ActionVerb for ActionSetDefaultPriority {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state
            .rt
            .priorities
            .set_mut(&self.set, &rules.attack_priorities)
            .default = self.priority;
        Ok(())
    }
}

struct ActionTeamApplyPriority {
    team: String,
    side: String,
    set: String,
}

impl ActionVerb for ActionTeamApplyPriority {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(team) = rules.get_team(state, &self.team, &self.side, false) else {
            return Ok(());
        };
        let members = rules.live_members(tick, state, team);
        let Some(proto) = state.rt.teams.get(team).map(|instance| instance.proto) else {
            return Ok(());
        };
        let stamp = state.rt.teams.stamp();
        state.rt.priorities.apply_to_team(
            proto,
            &members,
            &self.set,
            stamp,
            &rules.attack_priorities,
        );
        Ok(())
    }
}

struct ActionNamedApplyPriority {
    unit: String,
    side: String,
    set: String,
}

impl ActionVerb for ActionNamedApplyPriority {
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
        if let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) {
            let stamp = state.rt.teams.stamp();
            state
                .rt
                .priorities
                .apply_to_unit(entity, &self.set, stamp, &rules.attack_priorities);
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

pub(super) fn parse_objectlist_add(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionObjectListEdit {
        list: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        add: true,
    }))
}

pub(super) fn parse_objectlist_remove(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionObjectListEdit {
        list: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        add: false,
    }))
}

pub(super) fn parse_set_priority_thing(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetPriorityThing {
        set: parameter_string(params, 0)?,
        thing: parameter_string(params, 1)?,
        priority: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_set_priority_kind_of(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetPriorityKindOf {
        set: parameter_string(params, 0)?,
        kind_of: parameter_string(params, 1)?,
        priority: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_set_default_priority(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetDefaultPriority {
        set: parameter_string(params, 0)?,
        priority: parameter_int(params, 1)?,
    }))
}

pub(super) fn parse_team_apply_priority(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamApplyPriority {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        set: parameter_string(params, 1)?,
    }))
}

pub(super) fn parse_named_apply_priority(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNamedApplyPriority {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        set: parameter_string(params, 1)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entities(n: usize) -> Vec<Entity> {
        let mut world = World::new();
        (0..n).map(|_| world.spawn().unwrap()).collect()
    }

    fn holder(entity: Entity, joined_at: u64, proto: Option<usize>) -> SetHolder<'static> {
        SetHolder {
            entity,
            joined_at,
            team_proto: proto,
            module_set: None,
        }
    }

    #[test]
    fn a_unit_joining_a_team_takes_the_teams_set() {
        let [early, late, bare] = <[Entity; 3]>::try_from(entities(3)).unwrap();
        let mut data = AttackPriorityData::default();
        data.sets.insert(
            "archer".into(),
            PrioritySet {
                default: 35,
                ..PrioritySet::default()
            },
        );
        let mut sets = AttackPriorities::default();
        sets.apply_to_unit(late, "archer", 1, &data);
        sets.apply_to_team(7, &[early], "archer", 2, &data);
        let default_of = |h| sets.set_of(h, &data).map(|set| set.default);
        assert_eq!(default_of(holder(early, 1, Some(7))), Some(35));
        assert_eq!(default_of(holder(late, 3, Some(7))), Some(35));
        assert_eq!(default_of(holder(late, 3, Some(8))), Some(35));
        assert_eq!(default_of(holder(bare, 0, None)), None);
    }

    #[test]
    fn an_unknown_set_name_resolves_to_the_default_set() {
        let [unit] = <[Entity; 1]>::try_from(entities(1)).unwrap();
        let data = AttackPriorityData::default();
        let mut sets = AttackPriorities::default();
        sets.apply_to_unit(unit, "NoSuchSet", 1, &data);
        assert_eq!(
            sets.set_of(holder(unit, 0, None), &data).map(|s| s.default),
            Some(1)
        );
    }
}
