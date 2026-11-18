use std::collections::{BTreeMap, BTreeSet};

use mint_sage_map::{TeamDef, TeamTemplate};
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::counters::SideKey;
use super::{MapScriptEngine, MapScriptState};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};
use crate::templates::KindTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TeamId(u32);

impl TeamId {
    pub(crate) const fn new(id: u32) -> Self {
        Self(id)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

pub(crate) const THIS_TEAM: &str = "<This Team>";

#[derive(Debug, Clone)]
pub(crate) struct TeamProto {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) singleton: bool,
    pub(crate) template: TeamTemplate,
}

impl TeamProto {
    pub(crate) fn is_default(&self) -> bool {
        self.name
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("team"))
            && self
                .name
                .get(4..)
                .is_some_and(|side| side.eq_ignore_ascii_case(&self.owner))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TeamProtos(Vec<TeamProto>);

impl TeamProtos {
    pub(crate) fn from_defs(defs: &[TeamDef]) -> mint_sage_map::Result<Self> {
        let mut protos = Vec::with_capacity(defs.len());
        for def in defs {
            protos.push(TeamProto {
                owner: def.owner.clone(),
                name: def.name.clone(),
                singleton: def.singleton,
                template: def.template()?,
            });
        }
        Ok(Self(protos))
    }

    pub(crate) fn find(&self, owner: &str, name: &str) -> Option<usize> {
        self.0
            .iter()
            .position(|p| p.owner == owner && p.name == name)
    }

    pub(crate) fn get(&self, index: usize) -> Option<&TeamProto> {
        self.0.get(index)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, &TeamProto)> {
        self.0.iter().enumerate()
    }
}

pub(crate) trait TeamProbe {
    fn is_live(&self, entity: Entity) -> bool;
    fn effectively_dead(&self, entity: Entity) -> bool;
    fn forming(&self, entity: Entity) -> bool;
    fn unit_count(&self, _entity: Entity) -> i32 {
        1
    }
    fn has_kind(&self, _entity: Entity, _kind: &str) -> bool {
        false
    }
}

const NOT_UNITS: [&str; 3] = ["STRUCTURE", "PROJECTILE", "IGNORE_FOR_VICTORY"];

const NOT_SURVIVORS: [&str; 3] = ["PROJECTILE", "INERT", "IGNORE_FOR_VICTORY"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TeamHook {
    OnCreate,
    EnemySighted,
    AllClear,
    OnDestroyed,
    OnIdle,
    OnUnitDestroyed,
}

impl TeamHook {
    pub(crate) fn script(self, template: &TeamTemplate) -> Option<&str> {
        match self {
            Self::OnCreate => template.on_create_script.as_deref(),
            Self::EnemySighted => template.enemy_sighted_script.as_deref(),
            Self::AllClear => template.all_clear_script.as_deref(),
            Self::OnDestroyed => template.on_destroyed_script.as_deref(),
            Self::OnIdle => template.on_idle_script.as_deref(),
            Self::OnUnitDestroyed => template.on_unit_destroyed_script.as_deref(),
        }
    }
}

pub(crate) fn watches_for_enemies(template: &TeamTemplate) -> bool {
    template.enemy_sighted_script.is_some() || template.all_clear_script.is_some()
}

pub(crate) fn enemy_scan_due(team: TeamId, frame: u64) -> bool {
    (u64::from(team.0) ^ frame) & 7 == 0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeamInstance {
    pub(crate) id: TeamId,
    pub(crate) proto: usize,
    pub(crate) active: bool,
    pub(crate) created: bool,
    pub(crate) activated: bool,
    pub(crate) had_member: bool,
    pub(crate) entered_or_exited: bool,
    pub(crate) wait_for_build: bool,
    pub(crate) hold_create: bool,
    members: Vec<Entity>,
    generic_stopped: u32,
    generic_next: BTreeMap<usize, u64>,
    see_enemy: bool,
    prev_see_enemy: bool,
    cur_units: i32,
    destroy_threshold: i32,
    was_idle: bool,
}

impl TeamInstance {
    fn new(id: TeamId, proto: usize, active: bool) -> Self {
        Self {
            id,
            proto,
            active,
            created: active,
            activated: false,
            had_member: false,
            entered_or_exited: false,
            wait_for_build: false,
            hold_create: false,
            members: Vec::new(),
            generic_stopped: 0,
            generic_next: BTreeMap::new(),
            see_enemy: false,
            prev_see_enemy: false,
            cur_units: 0,
            destroy_threshold: 0,
            was_idle: false,
        }
    }

    pub(crate) fn members(&self) -> &[Entity] {
        &self.members
    }

    pub(crate) fn generic_tried(&self, slot: usize) -> bool {
        self.generic_stopped & super::generic::slot_bit(slot) == 0
    }

    pub(crate) fn generic_next(&self, slot: usize) -> u64 {
        self.generic_next.get(&slot).copied().unwrap_or(0)
    }

    pub(crate) fn set_active(&mut self) {
        if !self.active {
            self.active = true;
            self.created = true;
        }
    }

    pub(crate) fn has_any_units(&self, probe: &dyn TeamProbe) -> bool {
        self.any_member_but(probe, &NOT_UNITS)
    }

    fn any_member_but(&self, probe: &dyn TeamProbe, excluded: &[&str]) -> bool {
        self.members.iter().any(|&m| {
            probe.is_live(m)
                && !probe.effectively_dead(m)
                && !excluded.iter().any(|kind| probe.has_kind(m, kind))
        })
    }

    fn build_ready(&self, probe: &dyn TeamProbe) -> bool {
        !self.wait_for_build || !self.members.iter().any(|&m| probe.forming(m))
    }

    fn update_state(
        &mut self,
        probe: &dyn TeamProbe,
        template: Option<&TeamTemplate>,
        sees: Option<bool>,
        idle: Option<TeamIdle>,
    ) -> Vec<TeamHook> {
        self.entered_or_exited = false;
        let mut hooks = Vec::new();
        if !self.active {
            return hooks;
        }
        let on_destroyed = template.is_some_and(|t| t.on_destroyed_script.is_some());
        if self.created && !self.hold_create && self.build_ready(probe) {
            self.created = false;
            self.activated = true;
            hooks.push(TeamHook::OnCreate);
            if on_destroyed {
                for &member in &self.members {
                    self.cur_units = self.cur_units.saturating_add(probe.unit_count(member));
                }
                let threshold = template.map_or(0.0, |t| t.destroyed_threshold);
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let at = {
                    let units = self.cur_units as f32;
                    (units - units * threshold) as i32
                };
                self.destroy_threshold = at.min(self.cur_units.saturating_sub(1)).max(0);
            }
        }
        if !self.activated {
            return hooks;
        }
        if !self.had_member && !self.members.is_empty() {
            self.had_member = true;
        }
        if let Some(sees) = sees {
            self.prev_see_enemy = self.see_enemy;
            self.see_enemy = sees;
            if self.prev_see_enemy != self.see_enemy {
                hooks.push(if sees {
                    TeamHook::EnemySighted
                } else {
                    TeamHook::AllClear
                });
            }
        }
        if on_destroyed {
            let before = self.cur_units;
            self.cur_units = self
                .members
                .iter()
                .filter(|&&m| !probe.effectively_dead(m))
                .fold(0i32, |sum, &m| sum.saturating_add(probe.unit_count(m)));
            if self.cur_units != before && self.cur_units <= self.destroy_threshold {
                hooks.push(TeamHook::OnDestroyed);
                self.destroy_threshold = -1;
            }
        }
        if let Some(idle) = idle {
            if idle.any_alive && idle.all_idle && self.was_idle {
                hooks.push(TeamHook::OnIdle);
            }
            self.was_idle = idle.all_idle;
        }
        hooks
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TeamIdle {
    pub(crate) any_alive: bool,
    pub(crate) all_idle: bool,
}

enum Lookup {
    Found(TeamId),
    Inactive(TeamId),
    Create(usize),
    Missing,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TeamScope {
    pub(crate) calling: Option<TeamId>,
    pub(crate) condition: Option<TeamId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Teams {
    instances: BTreeMap<TeamId, TeamInstance>,
    next_id: u32,
    refs: BTreeMap<SideKey, TeamId>,
    by_member: BTreeMap<Entity, TeamId>,
    joins: BTreeMap<Entity, u64>,
    join_seq: u64,
    generic_active: BTreeMap<usize, u32>,
    controllers: BTreeMap<usize, PlayerId>,
    deaths_told: BTreeSet<Entity>,
}

impl Teams {
    pub(crate) fn controller(&self, proto: usize) -> Option<PlayerId> {
        self.controllers.get(&proto).copied()
    }

    pub(super) fn set_controller(&mut self, proto: usize, player: PlayerId) {
        self.controllers.insert(proto, player);
    }

    pub(crate) fn seeded(protos: &TeamProtos) -> Self {
        let mut teams = Self::default();
        for (index, proto) in protos.iter() {
            if proto.singleton {
                teams.create_instance(index, proto.is_default());
            }
        }
        teams
    }

    pub(crate) fn create_instance(&mut self, proto: usize, active: bool) -> TeamId {
        self.next_id = self.next_id.saturating_add(1);
        let id = TeamId(self.next_id);
        self.instances
            .insert(id, TeamInstance::new(id, proto, active));
        id
    }

    pub(crate) fn get(&self, id: TeamId) -> Option<&TeamInstance> {
        self.instances.get(&id)
    }

    pub(crate) fn generic_snapshot(&mut self, proto: usize, compute: impl FnOnce() -> u32) -> u32 {
        *self.generic_active.entry(proto).or_insert_with(compute)
    }

    pub(crate) fn stop_generic(&mut self, team: TeamId, slot: usize) {
        if let Some(instance) = self.instances.get_mut(&team) {
            instance.generic_stopped |= super::generic::slot_bit(slot);
        }
    }

    pub(crate) fn set_generic_next(&mut self, team: TeamId, slot: usize, frame: u64) {
        if let Some(instance) = self.instances.get_mut(&team) {
            instance.generic_next.insert(slot, frame);
        }
    }

    pub(crate) fn get_mut(&mut self, id: TeamId) -> Option<&mut TeamInstance> {
        self.instances.get_mut(&id)
    }

    pub(crate) fn instances(&self) -> impl Iterator<Item = &TeamInstance> {
        self.instances.values()
    }

    pub(crate) fn instances_of(&self, proto: usize) -> Vec<TeamId> {
        self.instances
            .values()
            .filter(|i| i.proto == proto)
            .map(|i| i.id)
            .collect()
    }

    pub(super) fn set_reference(&mut self, key: SideKey, team: TeamId) {
        self.refs.insert(key, team);
    }

    pub(crate) fn add_member(&mut self, team: TeamId, entity: Entity) {
        self.join_seq = self.join_seq.saturating_add(1);
        self.joins.insert(entity, self.join_seq);
        if let Some(old) = self.by_member.remove(&entity) {
            if let Some(instance) = self.instances.get_mut(&old) {
                instance.members.retain(|&m| m != entity);
            }
        }
        if let Some(instance) = self.instances.get_mut(&team) {
            instance.members.push(entity);
            self.by_member.insert(entity, team);
        }
    }

    pub(crate) fn adopt(&mut self, team: TeamId, entity: Entity) {
        if self.by_member.contains_key(&entity) {
            return;
        }
        if let Some(instance) = self.instances.get_mut(&team) {
            instance.members.push(entity);
            self.by_member.insert(entity, team);
            self.joins.entry(entity).or_insert(0);
        }
    }

    pub(crate) const fn stamp(&self) -> u64 {
        self.join_seq
    }

    pub(crate) fn joined_at(&self, entity: Entity) -> u64 {
        self.joins.get(&entity).copied().unwrap_or(0)
    }

    pub(crate) fn proto_of(&self, entity: Entity) -> Option<usize> {
        self.joins
            .contains_key(&entity)
            .then(|| self.team_of(entity))
            .flatten()
            .and_then(|id| self.instances.get(&id))
            .map(|i| i.proto)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.joins.retain(|entity, _| is_live(*entity));
    }

    pub(crate) fn team_of(&self, entity: Entity) -> Option<TeamId> {
        self.by_member.get(&entity).copied()
    }

    fn lookup(&self, protos: &TeamProtos, scope: TeamScope, name: &str, side: &str) -> Lookup {
        if name == THIS_TEAM {
            return scope
                .calling
                .or(scope.condition)
                .map_or(Lookup::Missing, Lookup::Found);
        }
        let key = SideKey::qualify(name, side);
        for team in [scope.calling, scope.condition].into_iter().flatten() {
            let named_by_proto = self
                .instances
                .get(&team)
                .and_then(|i| protos.get(i.proto))
                .is_some_and(|p| p.owner == key.side && p.name == name);
            if named_by_proto {
                return Lookup::Found(team);
            }
        }
        if let Some(&id) = self.refs.get(&key) {
            return if self.instances.contains_key(&id) {
                Lookup::Found(id)
            } else {
                Lookup::Missing
            };
        }
        let Some(proto_index) = protos.find(&key.side, &key.name) else {
            return Lookup::Missing;
        };
        let singleton = protos.get(proto_index).is_some_and(|p| p.singleton);
        let first = self.instances_of(proto_index).first().copied();
        match (first, singleton) {
            (Some(id), true) if self.instances.get(&id).is_some_and(|i| !i.active) => {
                Lookup::Inactive(id)
            }
            (Some(id), _) => Lookup::Found(id),
            (None, false) => Lookup::Create(proto_index),
            (None, true) => Lookup::Missing,
        }
    }

    pub(crate) fn peek_team_named(
        &self,
        protos: &TeamProtos,
        scope: TeamScope,
        name: &str,
        side: &str,
    ) -> Option<TeamId> {
        match self.lookup(protos, scope, name, side) {
            Lookup::Found(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn get_team_named(
        &mut self,
        protos: &TeamProtos,
        scope: TeamScope,
        name: &str,
        side: &str,
        activate: bool,
    ) -> Option<TeamId> {
        match self.lookup(protos, scope, name, side) {
            Lookup::Found(id) => Some(id),
            Lookup::Inactive(id) if activate => {
                self.instances.get_mut(&id)?.set_active();
                Some(id)
            }
            Lookup::Create(proto) if activate => Some(self.create_instance(proto, true)),
            _ => None,
        }
    }

    pub(crate) fn is_destroyed(&self, team: TeamId, probe: &dyn TeamProbe) -> bool {
        self.instances
            .get(&team)
            .is_some_and(|i| i.had_member && !i.any_member_but(probe, &NOT_SURVIVORS))
    }

    pub(crate) fn update_states(
        &mut self,
        probe: &dyn TeamProbe,
        protos: &TeamProtos,
        sight: &BTreeMap<TeamId, bool>,
    ) -> Vec<(TeamId, TeamHook)> {
        self.update_states_with(probe, protos, sight, &BTreeMap::new())
    }

    pub(crate) fn update_states_with(
        &mut self,
        probe: &dyn TeamProbe,
        protos: &TeamProtos,
        sight: &BTreeMap<TeamId, bool>,
        idle: &BTreeMap<TeamId, TeamIdle>,
    ) -> Vec<(TeamId, TeamHook)> {
        let mut due = Vec::new();
        self.deaths_told.retain(|&m| probe.is_live(m));
        for instance in self.instances.values_mut() {
            let by_member = &mut self.by_member;
            instance.members.retain(|&m| {
                let live = probe.is_live(m);
                if !live {
                    by_member.remove(&m);
                }
                live
            });
            let template = protos.get(instance.proto).map(|proto| &proto.template);
            for &member in &instance.members {
                if probe.effectively_dead(member) && self.deaths_told.insert(member) {
                    due.push((instance.id, TeamHook::OnUnitDestroyed));
                }
            }
            let sees = sight.get(&instance.id).copied();
            let idle = idle.get(&instance.id).copied();
            for hook in instance.update_state(probe, template, sees, idle) {
                due.push((instance.id, hook));
            }
        }
        due
    }

    pub(crate) fn disband(&mut self, team: TeamId, default: Option<TeamId>) {
        let Some(members) = self.instances.get(&team).map(|i| i.members.clone()) else {
            return;
        };
        match default {
            Some(default) if default == team => {}
            Some(default) => {
                for member in members {
                    self.add_member(default, member);
                }
            }
            None => {
                if let Some(instance) = self.instances.get_mut(&team) {
                    instance.members.clear();
                }
                for member in members {
                    self.by_member.remove(&member);
                }
            }
        }
    }

    pub(crate) fn hand_horde_to_soldiers(&mut self, horde: Entity, soldiers: &[Entity]) {
        let Some(team) = self.by_member.remove(&horde) else {
            return;
        };
        let Some(instance) = self.instances.get_mut(&team) else {
            return;
        };
        instance.members.retain(|&m| m != horde);
        for &soldier in soldiers {
            if self.by_member.contains_key(&soldier) {
                continue;
            }
            instance.members.push(soldier);
            self.by_member.insert(soldier, team);
            self.deaths_told.insert(soldier);
        }
    }

    pub(super) fn delete_instance(&mut self, id: TeamId) {
        self.instances.remove(&id);
        self.refs.retain(|_, held| *held != id);
    }

    pub(crate) fn remove_empty(&mut self, protos: &TeamProtos) -> Vec<TeamId> {
        let gone: Vec<TeamId> = self
            .instances
            .values()
            .filter(|i| {
                i.active
                    && i.members.is_empty()
                    && protos.get(i.proto).is_some_and(|p| !p.singleton)
            })
            .map(|i| i.id)
            .collect();
        for id in &gone {
            self.instances.remove(id);
        }
        gone
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.next_id);
        hasher.write_usize(self.instances.len());
        for i in self.instances.values() {
            hasher.write_u32(i.id.0);
            hasher.write_usize(i.proto);
            for flag in [
                i.active,
                i.created,
                i.activated,
                i.had_member,
                i.entered_or_exited,
                i.wait_for_build,
                i.hold_create,
            ] {
                hasher.write_bool(flag);
            }
            hasher.write_usize(i.members.len());
            for &m in &i.members {
                m.hash_into(hasher);
            }
            if i.see_enemy || i.prev_see_enemy || i.cur_units != 0 || i.destroy_threshold != 0 {
                hasher.write_bool(i.see_enemy);
                hasher.write_bool(i.prev_see_enemy);
                hasher.write_i64(i64::from(i.cur_units));
                hasher.write_i64(i64::from(i.destroy_threshold));
            }
            if i.was_idle {
                hasher.write_u8(0x49);
            }
            if i.generic_stopped != 0 || !i.generic_next.is_empty() {
                hasher.write_u32(i.generic_stopped);
                for (slot, frame) in &i.generic_next {
                    hasher.write_usize(*slot);
                    hasher.write_u64(*frame);
                }
            }
        }
        hasher.write_usize(self.refs.len());
        for (key, team) in &self.refs {
            hasher.write_bytes(key.side.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(key.name.as_bytes());
            hasher.write_u32(team.0);
        }
        if !self.deaths_told.is_empty() {
            hasher.write_u8(0x44);
            hasher.write_usize(self.deaths_told.len());
            for entity in &self.deaths_told {
                entity.hash_into(hasher);
            }
        }
        if !self.joins.is_empty() {
            hasher.write_usize(self.joins.len());
            for (entity, at) in &self.joins {
                entity.hash_into(hasher);
                hasher.write_u64(*at);
            }
        }
        if !self.generic_active.is_empty() {
            hasher.write_usize(self.generic_active.len());
            for (proto, mask) in &self.generic_active {
                hasher.write_usize(*proto);
                hasher.write_u32(*mask);
            }
        }
        if !self.controllers.is_empty() {
            hasher.write_usize(self.controllers.len());
            for (proto, player) in &self.controllers {
                hasher.write_usize(*proto);
                hasher.write_u8(player.get());
            }
        }
    }
}

pub(super) fn condition_team_name(
    names: impl Iterator<Item = String>,
    side: &str,
    protos: &TeamProtos,
) -> Option<String> {
    let mut single = None;
    let mut multi = None;
    for name in names {
        let key = SideKey::qualify(&name, side);
        let Some(proto) = protos
            .find(&key.side, &key.name)
            .and_then(|i| protos.get(i))
        else {
            continue;
        };
        if proto.singleton || proto.template.max_instances < 2 {
            single = Some(name);
        } else if multi.is_none() {
            multi = Some(name);
        }
    }
    multi.or(single)
}

struct WorldProbe<'a> {
    world: &'a World,
    ids: Ids,
    kinds: &'a KindTable,
}

impl TeamProbe for WorldProbe<'_> {
    fn is_live(&self, entity: Entity) -> bool {
        self.world.is_live(entity)
    }

    fn effectively_dead(&self, entity: Entity) -> bool {
        self.world.contains(self.ids.doomed, entity)
    }

    fn forming(&self, entity: Entity) -> bool {
        self.world.contains(self.ids.construction, entity)
            || self
                .world
                .get(self.ids.horde, entity)
                .is_some_and(|horde| horde.members.is_empty())
    }

    fn unit_count(&self, entity: Entity) -> i32 {
        self.world.get(self.ids.horde, entity).map_or(1, |horde| {
            let live = horde
                .members
                .iter()
                .filter(|&&m| self.world.is_live(m) && !self.world.contains(self.ids.doomed, m))
                .count();
            i32::try_from(live).unwrap_or(i32::MAX)
        })
    }

    fn has_kind(&self, entity: Entity, kind: &str) -> bool {
        self.world
            .get(self.ids.kind, entity)
            .and_then(|held| self.kinds.get(*held))
            .is_some_and(|entry| entry.record.has_kind(kind))
    }
}

pub(crate) fn hand_retired_hordes_to_soldiers(
    tick: &mut Tick<'_>,
    ids: Ids,
    retired: &[(Entity, Vec<Entity>)],
) {
    let world = tick.state();
    let Some(anchor) = world
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(entity, _)| entity))
    else {
        return;
    };
    let retired: Vec<(Entity, Vec<Entity>)> = retired
        .iter()
        .map(|(horde, soldiers)| {
            let live = soldiers.iter().copied().filter(|&s| world.is_live(s));
            (*horde, live.collect())
        })
        .collect();
    if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
        for (horde, soldiers) in &retired {
            state.rt.teams.hand_horde_to_soldiers(*horde, soldiers);
        }
    }
}

pub(crate) fn note_object_count_changed(tick: &mut Tick<'_>, ids: Ids) {
    let frame = tick.frame().get() / super::TICKS_PER_SCRIPT_FRAME;
    let Some(anchor) = tick
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(entity, _)| entity))
    else {
        return;
    };
    if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
        state.rt.counts.note_object_count_changed(frame);
    }
}

impl RotwkRules {
    pub(crate) fn peek_team(
        &self,
        state: &MapScriptState,
        name: &str,
        side: &str,
    ) -> Option<TeamId> {
        let protos = self.map_scripts.as_ref()?.team_prototypes();
        let scope = TeamScope {
            calling: state.rt.calling_team,
            condition: state.rt.condition_team,
        };
        state.rt.teams.peek_team_named(protos, scope, name, side)
    }

    pub(crate) fn get_team(
        &self,
        state: &mut MapScriptState,
        name: &str,
        side: &str,
        activate: bool,
    ) -> Option<TeamId> {
        let protos = self.map_scripts.as_ref()?.team_prototypes();
        let scope = TeamScope {
            calling: state.rt.calling_team,
            condition: state.rt.condition_team,
        };
        state
            .rt
            .teams
            .get_team_named(protos, scope, name, side, activate)
    }

    pub(crate) fn team_has_units(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> bool {
        let probe = WorldProbe {
            world: tick.state(),
            ids,
            kinds: &self.kinds,
        };
        state
            .rt
            .teams
            .get(team)
            .is_some_and(|instance| instance.has_any_units(&probe))
    }

    pub(crate) fn team_destroyed(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> bool {
        let probe = WorldProbe {
            world: tick.state(),
            ids,
            kinds: &self.kinds,
        };
        state.rt.teams.is_destroyed(team, &probe)
    }

    fn team_idle(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        members: &[Entity],
    ) -> TeamIdle {
        let world = tick.state();
        let mut reading = TeamIdle {
            any_alive: false,
            all_idle: true,
        };
        for &member in members {
            if world.contains(ids.doomed, member) || !self.has_ai(world, ids, member) {
                continue;
            }
            reading.any_alive = true;
            if !self.sequence_member_idle(tick, ids, state, member) {
                reading.all_idle = false;
            }
        }
        reading
    }

    fn disband_support_teams(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
    ) {
        let world = tick.state();
        let support = |entity: Entity| {
            world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| entry.record.kind_of.iter().any(|k| k == "SUPPORT"))
        };
        let disbanding: Vec<TeamId> = state
            .rt
            .teams
            .instances()
            .filter(|team| team.active && team.activated)
            .filter(|team| {
                let mut ai = team
                    .members
                    .iter()
                    .copied()
                    .filter(|&m| !world.contains(ids.doomed, m) && self.has_ai(world, ids, m))
                    .peekable();
                ai.peek().is_some() && ai.all(support)
            })
            .map(|team| team.id)
            .collect();
        for team in disbanding {
            let default = self.default_team_of(engine, state, team);
            diag!("script: team {team:?} disbands - every AI member is SUPPORT");
            state.rt.teams.disband(team, default);
        }
    }

    pub(super) fn adopt_into_default_teams(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
    ) {
        let mut defaults: BTreeMap<PlayerId, TeamId> = BTreeMap::new();
        for (index, proto) in engine.team_protos.iter() {
            if !proto.is_default() {
                continue;
            }
            let Some(&team) = state.rt.teams.instances_of(index).first() else {
                continue;
            };
            if let Some(player) = self.team_owner(engine, state, team) {
                defaults.entry(player).or_insert(team);
            }
        }
        let world = tick.state();
        let Some(owners) = world.store(ids.owner).filter(|_| !defaults.is_empty()) else {
            return;
        };
        let mut moved: Vec<(TeamId, Entity, bool)> = Vec::new();
        for (entity, owner) in owners.iter() {
            let Some(&default) = defaults.get(&owner.0) else {
                continue;
            };
            if !world.is_live(entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.member, entity)
                || world.contains(ids.defect_revert, entity)
                || !world.contains(ids.kind, entity)
            {
                continue;
            }
            match state.rt.teams.team_of(entity) {
                None => moved.push((default, entity, false)),
                Some(team) if self.team_owner(engine, state, team) != Some(owner.0) => {
                    moved.push((default, entity, true));
                }
                Some(_) => {}
            }
        }
        for (team, entity, handed_over) in moved {
            if handed_over {
                state.rt.teams.add_member(team, entity);
            } else {
                state.rt.teams.adopt(team, entity);
            }
        }
    }

    pub(super) fn default_team_of(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        team: TeamId,
    ) -> Option<TeamId> {
        let owner = &engine
            .team_protos
            .get(state.rt.teams.get(team)?.proto)?
            .owner;
        let proto = engine.team_protos.find(owner, &format!("team{owner}"))?;
        state.rt.teams.instances_of(proto).first().copied()
    }

    pub(super) fn update_team_states(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let frame = Self::script_frame(tick);
        let sight: BTreeMap<TeamId, bool> = state
            .rt
            .teams
            .instances()
            .filter(|team| team.active && enemy_scan_due(team.id, frame))
            .filter(|team| {
                engine
                    .team_protos
                    .get(team.proto)
                    .is_some_and(|proto| watches_for_enemies(&proto.template))
            })
            .map(|team| {
                (
                    team.id,
                    self.team_sees_enemy(tick, ids, state, &team.members),
                )
            })
            .collect();
        let idle: BTreeMap<TeamId, TeamIdle> = state
            .rt
            .teams
            .instances()
            .filter(|team| team.active && team.activated)
            .filter(|team| {
                engine
                    .team_protos
                    .get(team.proto)
                    .is_some_and(|proto| proto.template.on_idle_script.is_some())
            })
            .map(|team| (team.id, self.team_idle(tick, ids, state, &team.members)))
            .collect();
        let due = {
            let probe = WorldProbe {
                world: tick.state(),
                ids,
                kinds: &self.kinds,
            };
            state
                .rt
                .teams
                .update_states_with(&probe, &engine.team_protos, &sight, &idle)
        };
        for (id, hook) in due {
            let Some(proto) = state
                .rt
                .teams
                .get(id)
                .and_then(|instance| engine.team_protos.get(instance.proto))
            else {
                continue;
            };
            if matches!(hook, TeamHook::OnCreate) {
                self.apply_slot_bonuses(tick, ids, state, id)?;
            }
            let Some(script) = hook.script(&proto.template).map(str::to_owned) else {
                continue;
            };
            if !matches!(hook, TeamHook::OnIdle) {
                diag!(
                    "script: team {:?} runs its {hook:?} script {script:?}",
                    proto.name
                );
            }
            let side = proto.owner.clone();
            state.rt.calling_team = Some(id);
            let outcome = self.run_script(tick, ids, engine, state, &script, &side, kills);
            state.rt.calling_team = None;
            outcome?;
        }
        self.disband_support_teams(tick, ids, engine, state);
        for gone in state.rt.teams.remove_empty(&engine.team_protos) {
            state.rt.relations.forget_team(gone);
        }
        let world = tick.state();
        state.rt.retain_live(|entity| world.is_live(entity));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use mint_sim::World;

    use super::*;

    #[derive(Default)]
    struct Probe {
        dead: BTreeSet<Entity>,
        gone: BTreeSet<Entity>,
        forming: BTreeSet<Entity>,
        kinds: BTreeMap<Entity, &'static str>,
    }

    impl TeamProbe for Probe {
        fn is_live(&self, e: Entity) -> bool {
            !self.gone.contains(&e)
        }
        fn effectively_dead(&self, e: Entity) -> bool {
            self.dead.contains(&e)
        }
        fn forming(&self, e: Entity) -> bool {
            self.forming.contains(&e)
        }
        fn has_kind(&self, e: Entity, kind: &str) -> bool {
            self.kinds.get(&e) == Some(&kind)
        }
    }

    fn proto(owner: &str, name: &str, singleton: bool) -> TeamProto {
        TeamProto {
            owner: owner.to_owned(),
            name: name.to_owned(),
            singleton,
            template: empty_template(),
        }
    }

    fn empty_template() -> TeamTemplate {
        let def = TeamDef {
            owner: String::new(),
            name: String::new(),
            singleton: false,
            properties: mint_sage_map::Dictionary::default(),
        };
        def.template().unwrap()
    }

    fn entities(n: usize) -> Vec<Entity> {
        let mut world = World::new();
        (0..n).map(|_| world.spawn().unwrap()).collect()
    }

    #[test]
    fn a_singleton_stays_invisible_until_something_activates_it() {
        let protos = TeamProtos(vec![proto("Side", "Solo", true)]);
        let mut teams = Teams::seeded(&protos);
        let scope = TeamScope::default();
        assert_eq!(
            teams.get_team_named(&protos, scope, "Solo", "Side", false),
            None
        );
        let id = teams.get_team_named(&protos, scope, "Side/Solo", "Other", true);
        assert!(id.is_some());
        assert_eq!(
            teams.get_team_named(&protos, scope, "Solo", "Side", false),
            id
        );
    }

    #[test]
    fn a_players_default_team_answers_a_lookup_from_the_start() {
        let protos = TeamProtos(vec![
            proto("Side", "teamSide", true),
            proto("Side", "teamOther", true),
        ]);
        let mut teams = Teams::seeded(&protos);
        let scope = TeamScope::default();
        assert!(teams
            .get_team_named(&protos, scope, "teamSide", "Side", false)
            .is_some());
        assert_eq!(
            teams.get_team_named(&protos, scope, "teamOther", "Side", false),
            None
        );
    }

    #[test]
    fn a_non_singleton_is_created_on_demand_and_the_first_instance_answers_afterwards() {
        let protos = TeamProtos(vec![proto("Side", "Wave", false)]);
        let mut teams = Teams::seeded(&protos);
        let scope = TeamScope::default();
        assert_eq!(
            teams.get_team_named(&protos, scope, "Wave", "Side", false),
            None
        );
        let first = teams
            .get_team_named(&protos, scope, "Wave", "Side", true)
            .unwrap();
        teams.create_instance(0, true);
        assert_eq!(
            teams.get_team_named(&protos, scope, "Wave", "Side", false),
            Some(first)
        );
    }

    #[test]
    fn a_reference_name_wins_over_the_prototype_and_a_gone_target_does_not_fall_through() {
        let protos = TeamProtos(vec![proto("Side", "Wave", false)]);
        let mut teams = Teams::seeded(&protos);
        let scope = TeamScope::default();
        let proto_team = teams.create_instance(0, true);
        teams.add_member(proto_team, entities(1)[0]);
        let referenced = teams.create_instance(0, true);
        teams.set_reference(SideKey::qualify("Wave", "Side"), referenced);
        assert_eq!(
            teams.get_team_named(&protos, scope, "Wave", "Side", false),
            Some(referenced)
        );
        let probe = Probe::default();
        let _ = teams.update_states(&probe, &protos, &BTreeMap::new());
        teams.remove_empty(&protos);
        assert!(teams.get(proto_team).is_some());
        assert_eq!(
            teams.get_team_named(&protos, scope, "Wave", "Side", false),
            None
        );
    }

    #[test]
    fn this_team_is_the_calling_team_else_the_condition_team() {
        let protos = TeamProtos(vec![proto("Side", "Wave", false)]);
        let mut teams = Teams::seeded(&protos);
        let (a, b) = (
            teams.create_instance(0, true),
            teams.create_instance(0, true),
        );
        let both = TeamScope {
            calling: Some(a),
            condition: Some(b),
        };
        assert_eq!(
            teams.get_team_named(&protos, both, THIS_TEAM, "Side", false),
            Some(a)
        );
        let cond = TeamScope {
            calling: None,
            condition: Some(b),
        };
        assert_eq!(
            teams.get_team_named(&protos, cond, THIS_TEAM, "Side", false),
            Some(b)
        );
        assert_eq!(
            teams.get_team_named(&protos, cond, "Wave", "Side", false),
            Some(b)
        );
    }

    #[test]
    fn destroyed_reads_false_until_the_team_has_had_a_member_since_activation() {
        let protos = TeamProtos(vec![proto("Side", "Solo", true)]);
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Solo", "Side", true)
            .unwrap();
        let mut probe = Probe::default();
        let _ = teams.update_states(&probe, &protos, &BTreeMap::new());
        assert!(!teams.is_destroyed(id, &probe));
        let members = entities(1);
        teams.add_member(id, members[0]);
        let _ = teams.update_states(&probe, &protos, &BTreeMap::new());
        assert!(!teams.is_destroyed(id, &probe));
        probe.dead.insert(members[0]);
        assert!(teams.is_destroyed(id, &probe));
    }

    #[test]
    fn a_structure_is_no_unit_but_keeps_its_team_from_destroyed_and_an_inert_object_the_reverse() {
        let protos = TeamProtos(vec![proto("Side", "teamSide", true)]);
        let mut teams = Teams::seeded(&protos);
        let id = teams.instances_of(0)[0];
        let members = entities(1);
        teams.add_member(id, members[0]);
        let mut probe = Probe::default();
        let _ = teams.update_states(&probe, &protos, &BTreeMap::new());
        for (kind, unit, destroyed) in [
            ("STRUCTURE", false, false),
            ("INERT", true, true),
            ("PROJECTILE", false, true),
            ("IGNORE_FOR_VICTORY", false, true),
            ("INFANTRY", true, false),
        ] {
            probe.kinds.insert(members[0], kind);
            let instance = teams.get(id).unwrap();
            assert_eq!(instance.has_any_units(&probe), unit, "{kind}");
            assert_eq!(teams.is_destroyed(id, &probe), destroyed, "{kind}");
        }
    }

    #[test]
    fn an_adopted_object_joins_before_every_script_value_and_an_explicit_member_stays_put() {
        let protos = TeamProtos(vec![
            proto("Side", "teamSide", true),
            proto("Side", "Wave", false),
        ]);
        let mut teams = Teams::seeded(&protos);
        let default = teams.instances_of(0)[0];
        let wave = teams.create_instance(1, true);
        let members = entities(2);
        teams.add_member(wave, members[0]);
        teams.adopt(default, members[0]);
        teams.adopt(default, members[1]);
        assert_eq!(teams.team_of(members[0]), Some(wave));
        assert_eq!(teams.team_of(members[1]), Some(default));
        assert_eq!(teams.joined_at(members[1]), 0);
        assert_eq!(teams.proto_of(members[1]), Some(0));
    }

    #[test]
    fn on_create_waits_for_a_member_still_under_construction() {
        let protos = TeamProtos(vec![proto("Side", "Solo", true)]);
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Solo", "Side", true)
            .unwrap();
        let members = entities(1);
        teams.add_member(id, members[0]);
        teams.get_mut(id).unwrap().wait_for_build = true;
        let mut probe = Probe::default();
        probe.forming.insert(members[0]);
        assert!(teams
            .update_states(&probe, &protos, &BTreeMap::new())
            .is_empty());
        assert!(!teams.get(id).unwrap().activated);
        probe.forming.clear();
        assert_eq!(
            teams.update_states(&probe, &protos, &BTreeMap::new()),
            [(id, TeamHook::OnCreate)]
        );
        assert!(teams.get(id).unwrap().activated);
        assert!(teams
            .update_states(&probe, &protos, &BTreeMap::new())
            .is_empty());
    }

    #[test]
    fn a_member_moves_between_teams_rather_than_joining_two() {
        let protos = TeamProtos(vec![proto("Side", "Wave", false)]);
        let mut teams = Teams::seeded(&protos);
        let (a, b) = (
            teams.create_instance(0, true),
            teams.create_instance(0, true),
        );
        let members = entities(1);
        teams.add_member(a, members[0]);
        teams.add_member(b, members[0]);
        assert_eq!(teams.team_of(members[0]), Some(b));
        assert!(teams.get(a).unwrap().members.is_empty());
    }

    fn hooked(sighted: bool, destroyed: bool) -> TeamProtos {
        let mut pairs = Vec::new();
        if sighted {
            pairs.push((
                String::from("teamEnemySightedScript"),
                mint_sage_map::Value::Str("Seen".into()),
            ));
            pairs.push((
                String::from("teamAllClearScript"),
                mint_sage_map::Value::Str("Clear".into()),
            ));
        }
        if destroyed {
            pairs.push((
                String::from("teamOnDestroyedScript"),
                mint_sage_map::Value::Str("Gone".into()),
            ));
            pairs.push((
                String::from("teamDestroyedThreshold"),
                mint_sage_map::Value::Float(0.5),
            ));
        }
        let def = TeamDef {
            owner: String::from("Side"),
            name: String::from("Watch"),
            singleton: true,
            properties: mint_sage_map::Dictionary::from_pairs(pairs),
        };
        TeamProtos(vec![TeamProto {
            owner: String::from("Side"),
            name: String::from("Watch"),
            singleton: true,
            template: def.template().unwrap(),
        }])
    }

    #[test]
    fn enemy_sighted_and_all_clear_fire_on_a_change_only() {
        let protos = hooked(true, false);
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Watch", "Side", true)
            .unwrap();
        teams.add_member(id, entities(1)[0]);
        let probe = Probe::default();
        let scan = |seen: bool| BTreeMap::from([(id, seen)]);
        let hooks = |teams: &mut Teams, seen| {
            teams
                .update_states(&probe, &protos, &scan(seen))
                .into_iter()
                .map(|(_, hook)| hook)
                .collect::<Vec<_>>()
        };
        assert_eq!(hooks(&mut teams, false), [TeamHook::OnCreate]);
        assert_eq!(hooks(&mut teams, true), [TeamHook::EnemySighted]);
        assert!(hooks(&mut teams, true).is_empty());
        assert_eq!(hooks(&mut teams, false), [TeamHook::AllClear]);
    }

    #[test]
    fn on_destroyed_fires_once_at_its_threshold() {
        let protos = hooked(false, true);
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Watch", "Side", true)
            .unwrap();
        let members = entities(2);
        teams.add_member(id, members[0]);
        teams.add_member(id, members[1]);
        let mut probe = Probe::default();
        let fired = |teams: &mut Teams, probe: &Probe| {
            teams
                .update_states(probe, &protos, &BTreeMap::new())
                .into_iter()
                .filter(|(_, hook)| *hook == TeamHook::OnDestroyed)
                .count()
        };
        assert_eq!(fired(&mut teams, &probe), 0);
        probe.dead.insert(members[0]);
        assert_eq!(fired(&mut teams, &probe), 1);
        probe.dead.insert(members[1]);
        assert_eq!(fired(&mut teams, &probe), 0);
    }

    fn watch_with(key: &str) -> TeamProtos {
        let pairs = vec![(String::from(key), mint_sage_map::Value::Str("Hook".into()))];
        let def = TeamDef {
            owner: String::from("Side"),
            name: String::from("Watch"),
            singleton: true,
            properties: mint_sage_map::Dictionary::from_pairs(pairs),
        };
        TeamProtos(vec![TeamProto {
            owner: String::from("Side"),
            name: String::from("Watch"),
            singleton: true,
            template: def.template().unwrap(),
        }])
    }

    #[test]
    fn on_idle_fires_from_the_second_idle_update_on() {
        let protos = watch_with("teamOnIdleScript");
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Watch", "Side", true)
            .unwrap();
        teams.add_member(id, entities(1)[0]);
        let probe = Probe::default();
        let idle = BTreeMap::from([(
            id,
            TeamIdle {
                any_alive: true,
                all_idle: true,
            },
        )]);
        let mut fired = || {
            teams
                .update_states_with(&probe, &protos, &BTreeMap::new(), &idle)
                .into_iter()
                .filter(|(_, hook)| *hook == TeamHook::OnIdle)
                .count()
        };
        assert_eq!([fired(), fired()], [0, 1]);
    }

    #[test]
    fn on_unit_destroyed_fires_once_per_member_death() {
        let protos = watch_with("teamOnUnitDestroyedScript");
        let mut teams = Teams::seeded(&protos);
        let id = teams
            .get_team_named(&protos, TeamScope::default(), "Watch", "Side", true)
            .unwrap();
        let member = entities(1)[0];
        teams.add_member(id, member);
        let mut probe = Probe::default();
        probe.dead.insert(member);
        let mut fired = || {
            teams
                .update_states(&probe, &protos, &BTreeMap::new())
                .into_iter()
                .filter(|(_, hook)| *hook == TeamHook::OnUnitDestroyed)
                .count()
        };
        assert_eq!([fired(), fired()], [1, 0]);
    }

    #[test]
    fn a_disbanded_team_hands_its_members_to_the_default_team() {
        let protos = TeamProtos(vec![
            proto("Side", "Strike", true),
            proto("Side", "teamSide", true),
        ]);
        let mut teams = Teams::seeded(&protos);
        let strike = teams.instances_of(0)[0];
        let default = teams.instances_of(1)[0];
        let member = entities(1)[0];
        teams.add_member(strike, member);
        teams.disband(strike, Some(default));
        assert_eq!(teams.team_of(member), Some(default));
    }
}
