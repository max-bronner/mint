use mint_movement::{MoverClass, Vec2};
use mint_sage_asset::KindOf;
use mint_sage_combat::{Point3, RadiusAffects, Relation, VictimRuleTable, WeaponId, WeaponStatus};
use mint_sim::module::DestroyRequests;
use mint_sim::{Effects, Entity, Frame, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{
    run_nuggets_for, run_nuggets_scoped, DamageEvent, FireResult, Foe, Judge, NuggetScope, Shot,
};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::fx::{FxEvent, FxId, FxPoint, FxSource};
use crate::spell_state::{DelayedHit, DelayedHits, PendingShot, PendingShots};
use crate::unit::{Kind, PathKind};

const APPROACH_POINT_STEP: f32 = 10.0;

const APPROACH_POINT_STEP_CAP: i64 = 1_000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedRules {
    special: Option<ObjectFilter>,
    scalars: Vec<(f32, ObjectFilter)>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedVictims {
    table: Vec<((usize, usize), ResolvedRules)>,
    any: Vec<bool>,
}

impl ResolvedVictims {
    pub(crate) fn build(table: &VictimRuleTable, weapons: usize) -> Self {
        let mut any = vec![false; weapons];
        let mut out = Vec::with_capacity(table.len());
        for (&(weapon, nugget), rules) in table.iter() {
            if let Some(flag) = any.get_mut(weapon) {
                *flag = true;
            }
            out.push((
                (weapon, nugget),
                ResolvedRules {
                    special: (!rules.special.is_empty())
                        .then(|| ObjectFilter::parse(&rules.special)),
                    scalars: rules
                        .scalars
                        .iter()
                        .map(|(factor, filter)| (*factor, ObjectFilter::parse(filter)))
                        .collect(),
                },
            ));
        }
        Self { table: out, any }
    }

    fn weapon_has_rules(&self, weapon: WeaponId) -> bool {
        self.any.get(weapon.index()).copied().unwrap_or(false)
    }

    fn get(&self, weapon: WeaponId, nugget: usize) -> Option<&ResolvedRules> {
        self.table
            .binary_search_by(|(key, _)| key.cmp(&(weapon.index(), nugget)))
            .ok()
            .and_then(|at| self.table.get(at))
            .map(|(_, rules)| rules)
    }

    pub(crate) fn len(&self) -> usize {
        self.table.len()
    }
}

pub(super) struct WorldJudge<'a> {
    rules: &'a RotwkRules,
    world: &'a World,
    ids: Ids,
    weapon: WeaponId,
    attacker: Entity,
    attacker_owner: Option<PlayerId>,
    attacker_kind: Option<Kind>,
    affects: RadiusAffects,
    has_rules: bool,
    source: Option<(Vec2, Vec2)>,
}

impl<'a> WorldJudge<'a> {
    pub(super) fn new(
        rules: &'a RotwkRules,
        world: &'a World,
        ids: Ids,
        weapon: WeaponId,
        attacker: Entity,
        owner_if_gone: Option<PlayerId>,
    ) -> Self {
        Self {
            rules,
            world,
            ids,
            weapon,
            attacker,
            attacker_owner: world
                .get(ids.owner, attacker)
                .map(|owner| owner.0)
                .or(owner_if_gone),
            attacker_kind: world.get(ids.kind, attacker).copied(),
            affects: rules
                .combat
                .weapons
                .get(weapon)
                .map_or_else(RadiusAffects::default, |template| template.radius_affects),
            has_rules: rules.victim_rules.weapon_has_rules(weapon),
            source: world
                .get(ids.body, attacker)
                .map(|body| (body.0.position, body.0.facing.to_vector())),
        }
    }

    fn inside_damage_arc(&self, nugget: usize, candidate: Entity) -> bool {
        let Some(mint_sage_combat::Nugget::Damage(damage)) = self
            .rules
            .combat
            .weapons
            .get(self.weapon)
            .and_then(|template| template.nuggets.get(nugget))
        else {
            return true;
        };
        if damage.arc.is_none() {
            return true;
        }
        let (Some((at, facing)), Some(body)) =
            (self.source, self.world.get(self.ids.body, candidate))
        else {
            return true;
        };
        super::knockback::inside_arc(
            facing,
            body.0.position.minus(at),
            damage.arc,
            damage.arc_inverted,
        )
    }

    fn relation(&self, candidate: Entity) -> (Relation, bool) {
        let owner = self.world.get(self.ids.owner, candidate).map(|o| o.0);
        match (self.attacker_owner, owner) {
            (Some(a), Some(b)) if self.world.contains(self.ids.owner, self.attacker) => (
                self.rules
                    .object_victim_relation(self.world, self.attacker, candidate),
                a == b,
            ),
            (Some(a), Some(b)) => (self.rules.alliances.relation(a, b), a == b),
            _ => (Relation::Neutral, false),
        }
    }
}

impl Judge for WorldJudge<'_> {
    fn judge(&self, nugget: usize, candidate: Entity, primary: bool) -> Option<f32> {
        if !self.inside_damage_arc(nugget, candidate) {
            return None;
        }
        if !self.has_rules && self.affects == RadiusAffects::default() {
            return (primary || candidate != self.attacker).then_some(1.0);
        }
        let (relation, same_player) = self.relation(candidate);
        if !primary {
            let similar = relation == Relation::Allies
                && self.attacker_kind.is_some()
                && self.world.get(self.ids.kind, candidate).copied() == self.attacker_kind;
            let kind_of = self
                .world
                .get(self.ids.shape, candidate)
                .map_or(KindOf::NONE, |shape| shape.kind_of);
            let relation_exempt = (kind_of.contains(KindOf::MINE)
                && kind_of.contains(KindOf::IMMOBILE))
                || (relation == Relation::Neutral && kind_of.contains(KindOf::LINKED_TO_FLAG))
                || self.rules.script_status_bit(
                    self.world,
                    self.ids,
                    candidate,
                    super::map_scripts::ScriptStatusBit::Targetable,
                );
            if !self.affects.admits(
                relation,
                candidate == self.attacker,
                similar,
                relation_exempt,
            ) {
                return None;
            }
        }
        if !self.has_rules {
            return Some(1.0);
        }
        let Some(rules) = self.rules.victim_rules.get(self.weapon, nugget) else {
            return Some(1.0);
        };
        let entry = self
            .world
            .get(self.ids.kind, candidate)
            .and_then(|kind| self.rules.kinds.get(*kind));
        let (kind_of, name): (&[String], &str) =
            entry.map_or((&[], ""), |e| (&e.record.kind_of, &e.record.name));
        let passes = |filter: &ObjectFilter| {
            filter.passes(kind_of, name)
                && filter.passes_relation(
                    same_player,
                    relation == Relation::Allies,
                    relation == Relation::Enemies,
                )
        };
        if rules
            .special
            .as_ref()
            .is_some_and(|special| !passes(special))
        {
            return None;
        }
        let scalar = rules
            .scalars
            .iter()
            .filter(|(_, filter)| passes(filter))
            .fold(1.0f32, |scalar, (factor, _)| scalar * *factor);
        Some(scalar)
    }
}

impl RotwkRules {
    pub(super) fn setup_weapon_fire(&mut self) {
        self.victim_rules = ResolvedVictims::build(&self.combat.victims, self.combat.weapons.len());
        let mut names: Vec<String> = self
            .combat
            .weapons
            .iter()
            .flat_map(|(_, template)| {
                template
                    .fire_fx
                    .iter()
                    .chain(&template.fire_flank_fx)
                    .chain(&template.pre_attack_fx)
            })
            .cloned()
            .collect();
        for index in 0..self.kinds.len() {
            let Some(entry) = u16::try_from(index)
                .ok()
                .and_then(|k| self.kinds.get(Kind(k)))
            else {
                continue;
            };
            for module in &entry.record.declared_modules {
                for field in &module.fields {
                    let keyword = field.keyword.to_ascii_uppercase();
                    if keyword.ends_with("FX") || keyword.ends_with("FXLIST") {
                        names.extend(field.args.iter().cloned());
                    }
                }
            }
        }
        names.extend(self.combat.damage_fx.fx_lists().map(str::to_owned));
        self.fx_names = crate::fx::FxNames::build(names.iter().map(String::as_str));
        self.weapon_fx = self
            .combat
            .weapons
            .iter()
            .map(|(_, template)| {
                template
                    .fire_fx
                    .as_deref()
                    .and_then(|name| self.fx_names.id(name))
            })
            .collect();
        self.weapon_pre_attack_fx = self
            .combat
            .weapons
            .iter()
            .map(|(_, template)| {
                template
                    .pre_attack_fx
                    .as_deref()
                    .and_then(|name| self.fx_names.id(name))
            })
            .collect();
        self.body_kinds = (0..self.kinds.len())
            .map(|index| {
                u16::try_from(index)
                    .ok()
                    .and_then(|k| self.kinds.get(Kind(k)))
                    .and_then(|entry| {
                        entry
                            .record
                            .declared_modules
                            .iter()
                            .rev()
                            .find(|module| module.name.to_ascii_uppercase().ends_with("BODY"))
                            .map(|module| mint_sage_combat::BodyKind::parse(&module.name))
                    })
                    .unwrap_or_default()
            })
            .collect();
        println!(
            "weapons: {} nugget(s) with victim rules; {} FXList name(s) the match can play",
            self.victim_rules.len(),
            self.fx_names.len()
        );
    }

    pub(crate) fn fx_events<'w>(&self, world: &'w World, frame: Frame) -> &'w [FxEvent] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.fx)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, events)| events.of(frame))
    }

    pub(crate) fn fx_id(&self, name: &str) -> Option<FxId> {
        self.fx_names.id(name)
    }

    pub(crate) fn fx_name(&self, id: FxId) -> Option<&str> {
        self.fx_names.name(id)
    }

    pub(crate) fn fx_list_names(&self) -> impl Iterator<Item = &str> {
        (0..self.fx_names.len())
            .filter_map(|at| u16::try_from(at).ok())
            .filter_map(|at| self.fx_names.name(FxId(at)))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: FxId,
        at: impl Into<FxPoint>,
        source: FxSource,
        caller: Entity,
        target: Option<Vec2>,
        victim: Option<Entity>,
    ) {
        self.emit_fx_with_play(tick, ids, list, at, source, caller, target, victim, false);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_fx_with_play(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: FxId,
        at: impl Into<FxPoint>,
        source: FxSource,
        caller: Entity,
        target: Option<Vec2>,
        victim: Option<Entity>,
        on_object: bool,
    ) {
        let at = at.into();
        let now = tick.frame();
        let Some(store) = tick.state_mut().store_mut(ids.fx) else {
            return;
        };
        if let Some((_, events)) = store.iter_mut().next() {
            events.push(
                now,
                FxEvent {
                    list,
                    x: at.x,
                    y: at.y,
                    z: at.z,
                    source,
                    caller,
                    target: target.map(|target| (target.x, target.y)),
                    victim,
                    on_object,
                },
            );
        }
    }

    pub(super) fn emit_fx_named(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: &str,
        at: impl Into<FxPoint>,
        source: FxSource,
        caller: Entity,
    ) {
        if let Some(id) = self.fx_names.id(list) {
            self.emit_fx(tick, ids, id, at, source, caller, None, None);
        }
    }

    pub(super) fn emit_fx_object_named(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: &str,
        at: impl Into<FxPoint>,
        source: FxSource,
        caller: Entity,
        secondary: Option<Entity>,
    ) {
        if let Some(id) = self.fx_names.id(list) {
            self.emit_fx_with_play(tick, ids, id, at, source, caller, None, secondary, true);
        }
    }

    pub(super) fn emit_fx_on_horde_or_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: &str,
        source: FxSource,
        entity: Entity,
    ) {
        let world = tick.state();
        let horde = if world.contains(ids.horde, entity) {
            Some(entity)
        } else {
            world.get(ids.member, entity).map(|member| member.horde)
        };
        let targets: Vec<Entity> = match horde.and_then(|horde| world.get(ids.horde, horde)) {
            Some(horde) => horde
                .members
                .iter()
                .copied()
                .filter(|&member| world.is_live(member))
                .collect(),
            None => vec![entity],
        };
        for target in targets {
            if let Some(at) = Self::fx_point_of(tick.state(), ids, target) {
                self.emit_fx_object_named(tick, ids, list, at, source, target, None);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_weapon_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        at: impl Into<FxPoint>,
        caller: Entity,
        firer: Entity,
        target: Option<Vec2>,
        victim: Option<Entity>,
    ) -> Result<()> {
        let flank_fx = match victim {
            Some(victim) if self.flanks(tick, ids, firer, victim)? => self
                .combat
                .weapons
                .get(weapon)
                .and_then(|template| template.fire_flank_fx.as_deref())
                .and_then(|name| self.fx_names.id(name)),
            _ => None,
        };
        let list = flank_fx.or_else(|| self.weapon_fx.get(weapon.index()).copied().flatten());
        if let Some(id) = list {
            let on_object = self.barrels_of(tick.state(), caller).is_empty();
            self.emit_fx_with_play(
                tick,
                ids,
                id,
                at,
                FxSource::Weapon,
                caller,
                target,
                victim,
                on_object,
            );
        }
        Ok(())
    }

    pub(super) fn emit_weapon_pre_attack_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        at: impl Into<FxPoint>,
        caller: Entity,
    ) {
        if let Some(id) = self
            .weapon_pre_attack_fx
            .get(weapon.index())
            .copied()
            .flatten()
        {
            self.emit_fx(tick, ids, id, at, FxSource::Weapon, caller, None, None);
        }
    }

    pub(super) fn emit_voice(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::voice::VoiceKind,
        target: Option<Entity>,
    ) {
        let was_attacking = self.is_attacking_self_or_horde(tick, ids, entity);
        self.emit_voice_with(tick, ids, entity, kind, target, was_attacking);
    }

    pub(super) fn emit_voice_with(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::voice::VoiceKind,
        target: Option<Entity>,
        was_attacking: bool,
    ) {
        self.push_voice(tick, ids, entity, kind, target, was_attacking, None);
    }

    pub(super) fn emit_power_voice(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        sound: std::sync::Arc<str>,
    ) {
        self.push_voice(
            tick,
            ids,
            entity,
            crate::voice::VoiceKind::SpecialPower,
            None,
            false,
            Some(sound),
        );
    }

    pub(super) fn emit_object_sound(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        sound: std::sync::Arc<str>,
    ) {
        self.push_voice(
            tick,
            ids,
            entity,
            crate::voice::VoiceKind::ObjectSound,
            None,
            false,
            Some(sound),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_voice(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::voice::VoiceKind,
        target: Option<Entity>,
        was_attacking: bool,
        sound: Option<std::sync::Arc<str>>,
    ) {
        let entity = tick
            .state()
            .get(ids.horde, entity)
            .and_then(|horde| {
                horde
                    .members
                    .iter()
                    .find(|&&member| tick.state().is_live(member))
                    .copied()
            })
            .unwrap_or(entity);
        let now = tick.frame();
        let Some(store) = tick.state_mut().store_mut(ids.voice) else {
            return;
        };
        if let Some((_, events)) = store.iter_mut().next() {
            events.push(
                now,
                crate::voice::VoiceEvent {
                    entity,
                    kind,
                    target,
                    was_attacking,
                    sound,
                },
            );
        }
    }

    pub(crate) fn damage_type_named(&self, name: &str) -> Option<mint_sage_combat::DamageTypeId> {
        self.combat.damage_types.lookup(name)
    }

    pub(crate) fn combat_facts<'w>(
        &self,
        world: &'w World,
    ) -> Option<&'w crate::combat_facts::CombatFacts> {
        let ids = self.ids?;
        world
            .store(ids.combat_facts)
            .and_then(|store| store.iter().next())
            .map(|(_, facts)| facts)
    }

    pub(crate) fn formation_of(&self, kind: Kind) -> Option<&mint_sage_formation::FormationTable> {
        let formation = self.kinds.get(kind)?.formation?;
        self.formations.get(usize::from(formation))
    }

    pub(crate) fn defeat_facts<'w>(&self, world: &'w World, frame: Frame) -> &'w [PlayerId] {
        let Some(ids) = self.ids else {
            return &[];
        };
        crate::defeat::defeat_facts(world, Self::defeat_ids(ids), frame)
    }

    pub(crate) fn voice_events<'w>(
        &self,
        world: &'w World,
        frame: Frame,
    ) -> &'w [crate::voice::VoiceEvent] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.voice)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, events)| events.of(frame))
    }

    pub(crate) fn voice_target(
        &self,
        world: &World,
        entity: Entity,
        local: PlayerId,
    ) -> Option<(Vec2, u16, bool, Option<PlayerId>)> {
        let ids = self.ids?;
        let position = world.get(ids.body, entity)?.0.position;
        let kind = world.get(ids.kind, entity)?.0;
        let visible = world.get(ids.seen, entity).is_none_or(|seen| {
            matches!(
                seen.visibility(local),
                mint_sage_shroud::ObjectVisibility::Clear
                    | mint_sage_shroud::ObjectVisibility::PartialClear
            )
        });
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        Some((position, kind, visible, owner))
    }

    pub(crate) fn fx_facts(
        &self,
        world: &World,
        entity: Entity,
        local: PlayerId,
    ) -> Option<crate::fx_present::ObjectFacts> {
        let ids = self.ids?;
        let kind = *world.get(ids.kind, entity)?;
        let record = &self.kinds.get(kind)?.record;
        let model_conditions = self
            .modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .map(mint_sage_modules::UpgradeState::model_condition_names)
            .unwrap_or_default();
        let invisible_to_local = self.stealth_look_hides(world, ids, local, entity);
        Some(crate::fx_present::ObjectFacts {
            template: record.name.clone(),
            kind_of: record.kind_of.clone(),
            model_conditions,
            invisible_to_local,
        })
    }

    pub(crate) fn voice_group(&self, world: &World, entity: Entity) -> Vec<Entity> {
        let Some(ids) = self.ids else {
            return vec![entity];
        };
        let horde_entity = if world.contains(ids.horde, entity) {
            Some(entity)
        } else {
            world.get(ids.member, entity).map(|member| member.horde)
        };
        let Some(horde) = horde_entity.and_then(|horde_entity| world.get(ids.horde, horde_entity))
        else {
            return vec![entity];
        };
        horde
            .members
            .iter()
            .copied()
            .filter(|&member| {
                let Some(kind) = world.get(ids.kind, member).copied() else {
                    return false;
                };
                self.kinds.get(kind).is_none_or(|entry| {
                    !entry
                        .record
                        .kind_of
                        .iter()
                        .any(|name| name == "IGNORED_IN_GUI")
                })
            })
            .collect()
    }

    pub(crate) fn voice_owner(&self, world: &World, entity: Entity) -> Option<PlayerId> {
        Some(world.get(self.ids?.owner, entity)?.0)
    }

    pub(crate) fn voice_target_facts(
        &self,
        world: &World,
        actor: Entity,
        target: Entity,
    ) -> Option<crate::voice_chain::TargetFacts> {
        let ids = self.ids?;
        let kind = *world.get(ids.kind, target)?;
        let entry = self.kinds.get(kind)?;
        let has = |flag: &str| entry.record.kind_of.iter().any(|name| name == flag);
        let contested = world
            .get(ids.owner, actor)
            .is_some_and(|owner| self.contests(world, ids, owner.0, target));
        Some(crate::voice_chain::TargetFacts {
            kind: kind.0,
            structure: has("STRUCTURE"),
            machine: has("MACHINE"),
            heal_pad: has("HEAL_PAD"),
            airborne: self.airborne_category(world, ids, target).is_some(),
            contested,
        })
    }

    pub(crate) fn move_voice_facts(
        &self,
        world: &World,
        entity: Entity,
    ) -> Option<crate::voice_chain::MoveVoiceFacts> {
        let ids = self.ids?;
        let owner = world.get(ids.owner, entity)?.0;
        let position = world.get(ids.body, entity)?.0.position;
        let goal = world.get(ids.path, entity)?.goal;
        let min_z_increase = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.min_z_increase_for_voice_move_to_higher_ground)
            .filter(|min| *min > 0.0);
        let heights = &self.passability.heights;
        let higher_ground = min_z_increase.is_some_and(|min| {
            let (Some(start), Some(end)) = (
                heights.height_at_point(position),
                heights.height_at_point(goal),
            ) else {
                return false;
            };
            end - start > min
        });
        Some(crate::voice_chain::MoveVoiceFacts {
            campness_start: self.campness_at(world, ids, position, owner),
            campness_goal: self.campness_at(world, ids, goal, owner),
            higher_ground,
            engaged: world.contains(ids.engaged, entity),
        })
    }

    fn campness_at(&self, world: &World, ids: Ids, point: Vec2, asking: PlayerId) -> f32 {
        let Some(store) = world.store(ids.body) else {
            return 0.0;
        };
        let mut total = 0.0_f32;
        for (candidate, body) in store.iter() {
            if world.contains(ids.member, candidate) {
                continue;
            }
            let Some(owner) = world.get(ids.owner, candidate).map(|o| o.0) else {
                continue;
            };
            if owner != asking && !self.allied(owner, asking) {
                continue;
            }
            let Some(entry) = world
                .get(ids.kind, candidate)
                .and_then(|kind| self.kinds.get(*kind))
            else {
                continue;
            };
            if !entry.record.has_kind("STRUCTURE") {
                continue;
            }
            let Some(value) = entry.record.campness_value.filter(|v| *v > 0.0) else {
                continue;
            };
            let Some(radius) = entry.record.campness_value_radius.filter(|r| *r > 0.0) else {
                continue;
            };
            if crate::templates::vision(&entry.record).is_none_or(|range| range <= 0.0) {
                continue;
            }
            let dx = body.0.position.x - point.x;
            let dy = body.0.position.y - point.y;
            if dx * dx + dy * dy <= radius * radius {
                total += value;
            }
        }
        total
    }

    pub(super) fn queue_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        shot: PendingShot,
    ) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let mut pending = tick
            .state()
            .get(ids.pending_shots, clock)
            .cloned()
            .unwrap_or_default();
        pending.0.push(shot);
        tick.state_mut().insert(ids.pending_shots, clock, pending)?;
        Ok(())
    }

    pub(super) fn step_modules(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
        grid: &crate::combat::TargetGrid,
        budgets: &mut super::PathBudgets,
    ) -> Result<Vec<Entity>> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(Vec::new());
        };
        let now = tick.frame();
        let mut log = mint_sage_modules::EffectLog::new();
        let mut destroy = DestroyRequests::new();
        let hordes: Vec<(Entity, (f32, f32, f32, f32))> = tick
            .state()
            .store(ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|horde| {
                let body = tick.state().get(ids.body, horde)?.0;
                Some((
                    horde,
                    (
                        body.position.x,
                        body.position.y,
                        Self::mover_radius(tick, ids, horde),
                        body.facing.radians(),
                    ),
                ))
            })
            .collect();
        let unattackable: Vec<(Entity, (f32, f32, f32, f32))> = tick
            .state()
            .store(ids.health)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, pool)| !pool.0.is_dead())
                    .filter_map(|(entity, _)| {
                        let shape = tick.state().get(ids.shape, entity)?;
                        if !shape
                            .kind_of
                            .contains(mint_sage_asset::KindOf::UNATTACKABLE)
                        {
                            return None;
                        }
                        let body = tick.state().get(ids.body, entity)?.0;
                        Some((
                            entity,
                            (
                                body.position.x,
                                body.position.y,
                                shape.radius,
                                body.facing.radians(),
                            ),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let positions = mint_sage_modules::Positions::from_pairs(
            living
                .iter()
                .map(|(foe, _)| {
                    let facing = tick
                        .state()
                        .get(ids.body, foe.entity)
                        .map_or(0.0, |body| body.0.facing.radians());
                    (
                        foe.entity,
                        (foe.position.x, foe.position.y, foe.radius, facing),
                    )
                })
                .chain(hordes)
                .chain(unattackable),
        );
        let ordered = mint_sage_modules::PlayerOrdered::from_entities(
            tick.state()
                .store(ids.player_order)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default(),
        );
        let movers = mint_sage_modules::Movers::from_pairs(living.iter().filter_map(|(foe, _)| {
            let mover = self.approach_mover(tick, ids, foe.entity);
            (mover != foe.entity).then_some((foe.entity, mover))
        }));
        let moving = mint_sage_modules::StillMoving::from_entities(
            tick.state()
                .store(ids.path)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default(),
        );
        let structures = mint_sage_modules::Structures::from_entities(
            living
                .iter()
                .filter(|(foe, _)| foe.is_structure)
                .map(|(foe, _)| foe.entity),
        );
        let immobile =
            mint_sage_modules::Immobile::from_entities(living.iter().filter_map(|(foe, _)| {
                let kind = tick.state().get(ids.kind, foe.entity)?;
                self.kinds
                    .get(*kind)
                    .is_some_and(|entry| entry.record.has_kind("IMMOBILE"))
                    .then_some(foe.entity)
            }));
        let horde_casters = mint_sage_modules::Hordes::from_entities(
            tick.state()
                .store(ids.horde)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default(),
        );
        let view_blocked = mint_sage_modules::ViewBlocked::from_entities(
            wiring
                .collect_pending_approach_checks(tick.state())
                .into_iter()
                .filter_map(|(caster, target)| {
                    let from = tick.state().get(ids.body, caster)?.0.position;
                    let to = tick.state().get(ids.body, target)?.0.position;
                    let radius = tick
                        .state()
                        .get(ids.shape, target)
                        .map_or(0.0, |shape| shape.radius);
                    self.view_blocked(tick, ids, from, to, radius)
                        .then_some(caster)
                }),
        );
        let model_conditions = mint_sage_modules::ModelConditions::from_pairs(
            tick.state()
                .store(wiring.state)
                .map(|store| {
                    store
                        .iter()
                        .map(|(entity, state)| {
                            let mut names = state.model_condition_names();
                            if moving.contains(self.approach_mover(tick, ids, entity)) {
                                names.push("MOVING".to_string());
                            }
                            (entity, mint_sage_modules::effects::names(names))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let neutral = self.neutral_spawners(tick.state(), ids);
        let building = mint_sage_modules::UnderConstruction::from_entities(
            tick.state()
                .store(ids.construction)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default(),
        );
        let slaves = self.slave_views(tick.state(), ids);
        let master_slaves = self.master_slaves(tick.state(), ids);
        self.log_slaves(tick.state(), ids, now);
        {
            let (world, rng) = tick.state_and_rng_mut();
            wiring.update(
                world,
                now,
                rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                &[
                    &positions,
                    &ordered,
                    &movers,
                    &moving,
                    &structures,
                    &immobile,
                    &horde_casters,
                    &view_blocked,
                    &neutral,
                    &building,
                    &slaves,
                    &master_slaves,
                    &model_conditions,
                ],
            );
        }
        for (entity, effect) in log.drain() {
            if self.apply_slave_effect(tick, ids, entity, &effect, budgets)? {
                continue;
            }
            if let mint_sage_modules::Effect::MoveToObject { target } = &effect {
                let mover = self.approach_mover(tick, ids, entity);
                let (Some(from), Some(to)) = (
                    tick.state().get(ids.body, mover).map(|b| b.0.position),
                    tick.state().get(ids.body, *target).map(|b| b.0.position),
                ) else {
                    continue;
                };
                let target_reach = Self::mover_radius(tick, ids, *target);
                self.close_on_target(
                    tick,
                    ids,
                    mover,
                    from,
                    to,
                    target_reach,
                    PathKind::Ability,
                    budgets,
                )?;
            } else if let mint_sage_modules::Effect::MoveToApproachPoint { target, max_range } =
                &effect
            {
                let mover = self.approach_mover(tick, ids, entity);
                let (Some(from), Some(to)) = (
                    tick.state().get(ids.body, mover).map(|b| b.0.position),
                    tick.state().get(ids.body, *target).map(|b| b.0.position),
                ) else {
                    continue;
                };
                let mover_class = self.mover_class(tick, ids, mover);
                let at = self.ability_approach_point(tick, ids, from, to, *max_range, mover_class);
                self.close_on_target(tick, ids, mover, from, at, 0.0, PathKind::Ability, budgets)?;
            } else if let mint_sage_modules::Effect::MoveToPosition { at } = &effect {
                let mover = self.approach_mover(tick, ids, entity);
                let Some(from) = tick.state().get(ids.body, mover).map(|b| b.0.position) else {
                    continue;
                };
                self.close_on_target(
                    tick,
                    ids,
                    mover,
                    from,
                    Vec2::new(at.0, at.1),
                    0.0,
                    PathKind::Ability,
                    budgets,
                )?;
            } else if matches!(effect, mint_sage_modules::Effect::Halt) {
                if self.approach_mover(tick, ids, entity) == entity {
                    tick.state_mut().remove(ids.path, entity);
                    if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                        body.0.speed = 0.0;
                        tick.state_mut().insert(ids.body, entity, body)?;
                    }
                }
            } else if let mint_sage_modules::Effect::FaceObject { target } = &effect {
                let Some(towards) = tick.state().get(ids.body, *target).map(|b| b.0.position)
                else {
                    continue;
                };
                Self::log_facing_debug(tick, ids, entity, towards, "facing");
                if self.approach_mover(tick, ids, entity) == entity {
                    self.turn_towards(tick, ids, entity, towards)?;
                }
            } else if let mint_sage_modules::Effect::FacePosition { at } = &effect {
                if self.approach_mover(tick, ids, entity) == entity {
                    self.turn_towards(tick, ids, entity, Vec2::new(at.0, at.1))?;
                }
            } else if let mint_sage_modules::Effect::StartRecharge(power) = &effect {
                self.begin_recharge(tick, ids, entity, power)?;
            } else if let mint_sage_modules::Effect::MarkSpecialPowerTriggered(power) = &effect {
                if !self.power_defers_recharge(tick, ids, entity, power) {
                    self.begin_recharge(tick, ids, entity, power)?;
                }
                self.trigger_special_power(tick, ids, entity, power, None)?;
            } else if let mint_sage_modules::Effect::Aura {
                list,
                radius,
                affects,
                sides,
                allow_self,
                lasts,
                while_dead,
            } = &effect
            {
                self.pulse_aura(
                    tick,
                    ids,
                    entity,
                    list,
                    *radius,
                    affects,
                    *sides,
                    *allow_self,
                    *lasts,
                    *while_dead,
                    living,
                    grid,
                )?;
            } else if let mint_sage_modules::Effect::AutoHeal {
                amount,
                radius,
                kinds,
                forbidden,
                skip_self,
                quiet,
                fx,
                sole_for,
                as_percent,
                nonstackable,
                skip_recently_damaged,
                fx_always,
            } = &effect
            {
                let pulse = super::regeneration::HealPulse {
                    amount: *amount,
                    radius: *radius,
                    kinds,
                    forbidden,
                    skip_self: *skip_self,
                    quiet: *quiet,
                    fx: fx.as_ref(),
                    sole_for: *sole_for,
                    as_percent: *as_percent,
                    nonstackable: *nonstackable,
                    skip_recently_damaged: *skip_recently_damaged,
                    fx_always: *fx_always,
                };
                self.pulse_auto_heal(tick, ids, entity, &pulse, living, grid)?;
            } else {
                if let mint_sage_modules::Effect::FireWeapon {
                    at: mint_sage_modules::PowerPlace::Object(target),
                    ..
                } = &effect
                {
                    if let Some(towards) = tick.state().get(ids.body, *target).map(|b| b.0.position)
                    {
                        Self::log_facing_debug(tick, ids, entity, towards, "fire");
                    }
                }
                self.apply_effects(tick, ids, vec![(entity, effect)])?;
            }
        }
        self.step_auto_casts(tick, ids, living, grid)?;
        self.step_ai_special_powers(tick, ids, living, grid)?;
        self.fire_pending(tick, ids, events, living)?;
        self.pulse_dots(tick, ids, events)?;
        self.drain_delayed(tick, ids, events, living)?;
        Ok(destroy.drain_sorted())
    }

    fn log_facing_debug(tick: &Tick<'_>, ids: Ids, entity: Entity, towards: Vec2, label: &str) {
        if std::env::var_os("MINT_ROTWK_FACING_DEBUG").is_none() {
            return;
        }
        let Some(body) = tick.state().get(ids.body, entity) else {
            return;
        };
        let Some(desired) = mint_movement::Facing::from_vector(towards.minus(body.0.position))
        else {
            return;
        };
        let bearing_off = body.0.facing.delta_to(desired).to_degrees();
        println!(
            "ability {label}: frame {} entity {:?} facing {:.1} deg, {:.1} deg off the target",
            tick.frame().get(),
            entity,
            body.0.facing.radians().to_degrees(),
            bearing_off
        );
    }

    pub(super) fn approach_mover(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Entity {
        if self.modes(tick, ids).formation != crate::horde::FormationMode::On {
            return entity;
        }
        tick.state()
            .get(ids.member, entity)
            .map(|member| member.horde)
            .filter(|horde| tick.state().contains(ids.horde, *horde))
            .unwrap_or(entity)
    }

    fn ability_approach_point(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        from: Vec2,
        to: Vec2,
        max_range: f32,
        mover: MoverClass,
    ) -> Vec2 {
        let toward_caster = from.minus(to);
        let length = toward_caster.length();
        if length.is_nan() || length <= 0.0 || !max_range.is_finite() {
            return to;
        }
        let step = toward_caster.scale(APPROACH_POINT_STEP / length);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let max_steps = ((max_range / APPROACH_POINT_STEP).floor() as i64 - 1)
            .clamp(0, APPROACH_POINT_STEP_CAP);
        for n in 1..=max_steps {
            #[allow(clippy::cast_precision_loss)]
            let point = to.plus(step.scale(n as f32));
            if self.passable_cell_for(tick, ids, point, mover).is_some() {
                return point;
            }
        }
        to
    }

    fn fire_pending(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(PendingShots(shots)) = tick.state_mut().remove(ids.pending_shots, clock) else {
            return Ok(());
        };
        let now = tick.frame();
        for shot in shots {
            let world = tick.state();
            if !world.is_live(shot.source) && shot.owner_if_gone.is_none() {
                continue;
            }
            let dying = world.contains(ids.doomed, shot.source)
                || world
                    .get(ids.health, shot.source)
                    .is_some_and(|h| h.0.is_dead());
            if shot.alive_only && dying {
                continue;
            }
            let Some(template) = self.combat.weapons.get(shot.weapon) else {
                continue;
            };
            if let Some(nugget) = shot.repeat {
                let mut clocks = world
                    .get(ids.weapon_clocks, shot.source)
                    .cloned()
                    .unwrap_or_default();
                let Some(clock) = clocks.clock(nugget, template) else {
                    continue;
                };
                if clock.status(now) != WeaponStatus::ReadyToFire {
                    continue;
                }
                clock.fire(template, now, tick.rng());
                tick.state_mut()
                    .insert(ids.weapon_clocks, shot.source, clocks)?;
            }
            if let Some(slot) = shot.reload_slot {
                let mut clocks = tick
                    .state()
                    .get(ids.ability_weapon_clocks, shot.source)
                    .cloned()
                    .unwrap_or_default();
                let Some(clock) = clocks.clock(slot, template) else {
                    continue;
                };
                if clock.status(now) != WeaponStatus::ReadyToFire {
                    continue;
                }
                clock.fire(template, now, tick.rng());
                tick.state_mut()
                    .insert(ids.ability_weapon_clocks, shot.source, clocks)?;
            }
            crate::diag::diag!(
                "shot: frame {} entity {} weapon {} at a place (module)",
                now.get(),
                shot.source.index(),
                shot.weapon.index()
            );
            let aimed_at = shot
                .victim
                .filter(|victim| !tick.state().contains(ids.horde, *victim))
                .and_then(|victim| self.foe_of(tick, ids, victim));
            match aimed_at {
                Some(foe) => self.fire_at_object(
                    tick,
                    ids,
                    shot.source,
                    shot.weapon,
                    &foe,
                    shot.owner_if_gone,
                    events,
                    living,
                )?,
                None => self.fire_at_place(
                    tick,
                    ids,
                    shot.source,
                    shot.weapon,
                    shot.at,
                    shot.owner_if_gone,
                    events,
                    living,
                )?,
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_at_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        weapon: WeaponId,
        target: &Foe,
        owner_if_gone: Option<PlayerId>,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let roll = self.roll_shot(tick, ids, weapon, target);
        let Some(launch) = self.module_launch(tick, ids, source, None) else {
            return self.fire_at_place(
                tick,
                ids,
                source,
                weapon,
                target.position,
                owner_if_gone,
                events,
                living,
            );
        };
        let shot = Shot {
            owner_if_gone,
            ..self.shot_at_foe(weapon, source, launch, target, roll.outcome)
        };
        diag!(
            "weapon: fired from ({:.0},{:.0},{:.0}) at entity {}",
            launch.x,
            launch.y,
            launch.z,
            target.entity.index()
        );
        let melee = self
            .combat
            .weapons
            .get(weapon)
            .is_some_and(|template| template.melee);
        let fx = if melee { target.entity } else { source };
        self.launch_shot(
            tick,
            ids,
            &shot,
            fx,
            target.position,
            roll.passenger_roll,
            living,
            events,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_at_place(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        weapon: WeaponId,
        at: Vec2,
        owner_if_gone: Option<PlayerId>,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        if let Some(launch) = self.module_launch(tick, ids, source, None) {
            let shot = Shot {
                owner_if_gone,
                ..Shot::at_ground(weapon, source, launch, at, self.ground_height(at.x, at.y))
            };
            diag!(
                "weapon: fired from ({:.0},{:.0},{:.0}) at the ground at ({:.0},{:.0})",
                launch.x,
                launch.y,
                launch.z,
                at.x,
                at.y
            );
            return self.launch_shot(tick, ids, &shot, source, at, None, living, events);
        }
        let shot = Shot {
            weapon,
            attacker: source,
            origin: Point3::new(at.x, at.y, self.ground_height(at.x, at.y)),
            victim: None,
            missed: None,
            centre: at,
            centre_height: self.ground_height(at.x, at.y),
            owner_if_gone,
        };
        let result = self.fire_shot(tick, ids, &shot, living)?;
        self.emit_weapon_fx(tick, ids, weapon, at, source, source, Some(at), None)?;
        if result.projectiles_unaimed > 0 {
            diag!(
                "weapon: {} projectile nugget(s) fired at a place with nothing to fly at",
                result.projectiles_unaimed
            );
        }
        self.apply_fire_result(tick, ids, weapon, &shot, result, events)
    }

    pub(super) fn module_launch(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        slot: Option<usize>,
    ) -> Option<Point3> {
        let world = tick.state();
        let position = world.get(ids.body, source)?.0.position;
        let shape = world.get(ids.shape, source)?;
        let me = Foe {
            entity: source,
            position,
            z: Self::feet(world, ids, source),
            radius: shape.radius,
            height: shape.height,
            is_structure: shape.kind_of.contains(KindOf::STRUCTURE),
            airborne: None,
            unattackable: false,
            not_autoacquirable: false,
            ai_kind_of: None,
        };
        Some(self.take_launch(tick, source, &me, slot))
    }

    pub(super) fn fire_shot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        shot: &Shot,
        living: &[(Foe, PlayerId)],
    ) -> Result<FireResult> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("fire_shot"));
        let held = self.shooter_upgrades(tick, ids, shot.weapon, shot.attacker);
        let travel = shot
            .victim
            .map_or(shot.centre, |(_, position, _)| position)
            .minus(Vec2::new(shot.origin.x, shot.origin.y))
            .length();
        let delay = self
            .combat
            .weapons
            .get(shot.weapon)
            .and_then(|template| template.damage_delay(travel, Self::ticks_per_second()));
        if let Some(delay) = delay {
            let due = tick.frame().checked_add(delay).unwrap_or(Frame::FOREVER);
            diag!(
                "weapon: fired on frame {}, {travel:.0} units of travel, damage due on {}",
                tick.frame().get(),
                due.get()
            );
            self.queue_delayed(
                tick,
                ids,
                DelayedHit {
                    source: shot.attacker,
                    weapon: shot.weapon,
                    victim: shot.victim.map(|(entity, ..)| entity),
                    at: shot.centre,
                    height: shot.centre_height,
                    due,
                    nugget: None,
                    owner_if_gone: shot.owner_if_gone,
                },
            )?;
            return Ok(FireResult::default());
        }
        let judge = WorldJudge::new(
            self,
            tick.state(),
            ids,
            shot.weapon,
            shot.attacker,
            shot.owner_if_gone,
        );
        Ok(run_nuggets_for(
            &self.combat,
            shot,
            living.iter().map(|(foe, _)| *foe),
            &held,
            &judge,
        ))
    }

    fn queue_delayed(&self, tick: &mut Tick<'_>, ids: Ids, hit: DelayedHit) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let mut queue = tick
            .state()
            .get(ids.delayed_hits, clock)
            .cloned()
            .unwrap_or_default();
        queue.0.push(hit);
        tick.state_mut().insert(ids.delayed_hits, clock, queue)?;
        Ok(())
    }

    fn drain_delayed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let now = tick.frame();
        let Some(queue) = tick
            .state()
            .get(ids.delayed_hits, clock)
            .filter(|queue| queue.0.iter().any(|hit| hit.due <= now))
            .cloned()
        else {
            return Ok(());
        };
        let (due, waiting): (Vec<DelayedHit>, Vec<DelayedHit>) =
            queue.0.into_iter().partition(|hit| hit.due <= now);
        if waiting.is_empty() {
            tick.state_mut().remove(ids.delayed_hits, clock);
        } else {
            tick.state_mut()
                .insert(ids.delayed_hits, clock, DelayedHits(waiting))?;
        }
        for hit in due {
            let victim = hit.victim.and_then(|entity| {
                living
                    .iter()
                    .find(|(foe, _)| foe.entity == entity)
                    .map(|(foe, _)| (foe.entity, foe.position, foe.aim_point().z))
            });
            let centre = victim.map_or(hit.at, |(_, position, _)| position);
            let shot = Shot {
                weapon: hit.weapon,
                attacker: hit.source,
                origin: Point3::new(centre.x, centre.y, hit.height),
                victim,
                missed: None,
                centre,
                centre_height: hit.height,
                owner_if_gone: hit.owner_if_gone,
            };
            diag!(
                "weapon: delayed damage due on frame {} lands on {}, victim {}",
                hit.due.get(),
                now.get(),
                if victim.is_some() { "alive" } else { "gone" }
            );
            let held = self.shooter_upgrades(tick, ids, hit.weapon, hit.source);
            let result = {
                let judge = WorldJudge::new(
                    self,
                    tick.state(),
                    ids,
                    hit.weapon,
                    hit.source,
                    hit.owner_if_gone,
                );
                run_nuggets_scoped(
                    &self.combat,
                    &shot,
                    living.iter().map(|(foe, _)| *foe),
                    &held,
                    &judge,
                    hit.nugget.map_or(NuggetScope::All, NuggetScope::Landing),
                )
            };
            self.apply_fire_result(tick, ids, hit.weapon, &shot, result, events)?;
        }
        Ok(())
    }

    pub(super) fn apply_fire_result(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        shot: &Shot,
        mut result: FireResult,
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        self.apply_effect_nuggets(tick, ids, weapon, shot, &result)?;
        let world = tick.state();
        let caught: Vec<Entity> = world
            .get(ids.passengers, shot.attacker)
            .map(|list| {
                list.0
                    .iter()
                    .copied()
                    .filter(|held| {
                        world
                            .get(ids.contained, *held)
                            .is_some_and(|held| held.grabbed_at.is_some())
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !caught.is_empty() {
            result.damage.retain(|hit| !caught.contains(&hit.victim));
            result.dots.retain(|hit| !caught.contains(&hit.victim));
            result
                .modifiers
                .retain(|(victim, _)| !caught.contains(victim));
            result
                .knockbacks
                .retain(|(victim, _)| !caught.contains(victim));
        }
        self.hit_passengers(tick, ids, weapon, &result.damage, events)?;
        events.extend(result.damage);
        let now = tick.frame();
        for nugget in result.delayed {
            let delay = match self
                .combat
                .weapons
                .get(weapon)
                .and_then(|template| template.nuggets.get(usize::from(nugget)))
            {
                Some(mint_sage_combat::Nugget::Damage(damage)) => damage.delay,
                Some(mint_sage_combat::Nugget::MetaImpact(impact)) => impact.delay,
                _ => continue,
            };
            let due = now.checked_add(delay).unwrap_or(Frame::FOREVER);
            diag!(
                "weapon: nugget {nugget} delayed from frame {} to {}",
                now.get(),
                due.get()
            );
            self.queue_delayed(
                tick,
                ids,
                DelayedHit {
                    source: shot.attacker,
                    weapon,
                    victim: shot.victim.map(|(entity, ..)| entity),
                    at: shot.centre,
                    height: shot.centre_height,
                    due,
                    nugget: Some(nugget),
                    owner_if_gone: shot.owner_if_gone,
                },
            )?;
        }
        let magic = self.combat.damage_types.lookup("MAGIC");
        for mut hit in result.dots {
            if let Some(bonuses) = tick.state().get(ids.bonuses, hit.attacker) {
                hit.amount = bonuses.0.damage_of(
                    hit.amount,
                    Some(hit.damage_type) == magic,
                    hit.accepts_add,
                );
            }
            let mut dots = tick
                .state()
                .get(ids.dots, hit.victim)
                .cloned()
                .unwrap_or_default();
            dots.hit(now, &hit);
            tick.state_mut().insert(ids.dots, hit.victim, dots)?;
        }
        self.begin_knockback(tick, ids, weapon, shot, &result.knockbacks)?;
        for (victim, nugget) in result.modifiers {
            let Some(mint_sage_combat::Nugget::Modifier(modifier)) = self
                .combat
                .weapons
                .get(weapon)
                .and_then(|template| template.nuggets.get(nugget))
            else {
                continue;
            };
            let Some(id) = self.combat.modifiers.id(&modifier.list) else {
                continue;
            };
            let until = self.list_end(now, id);
            self.give_timed_list(tick, ids, victim, id, until)?;
        }
        Ok(())
    }

    fn hit_passengers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        hits: &[DamageEvent],
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let chance = self
            .combat
            .weapons
            .get(weapon)
            .map_or(0.0, |template| template.hit_passenger_chance);
        if chance <= 0.0 {
            return Ok(());
        }
        for hit in hits {
            if self
                .first_passenger(tick.state(), ids, hit.victim)
                .is_none()
            {
                continue;
            }
            if tick.rng().next_f32() < chance {
                events.extend(self.passenger_hit(tick.state(), ids, hit));
            }
        }
        Ok(())
    }

    pub(super) fn passenger_hit(
        &self,
        world: &World,
        ids: Ids,
        hit: &DamageEvent,
    ) -> Option<DamageEvent> {
        let passenger = self.first_passenger(world, ids, hit.victim)?;
        diag!(
            "weapon: HitPassengerPercentage struck entity {} inside entity {}",
            passenger.index(),
            hit.victim.index()
        );
        Some(DamageEvent {
            victim: passenger,
            ..*hit
        })
    }

    fn apply_effect_nuggets(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: WeaponId,
        shot: &Shot,
        result: &FireResult,
    ) -> Result<()> {
        use mint_sage_combat::EffectNugget;

        let effect = |index: usize| self.combat.effects.get(weapon.index(), index);
        for &index in &result.creation_lists {
            let Some(EffectNugget::WeaponOcl(nugget)) = effect(index) else {
                continue;
            };
            let Some(owner) = tick.state().get(ids.owner, shot.attacker).map(|o| o.0) else {
                continue;
            };
            let firer_position = tick
                .state()
                .get(ids.body, shot.attacker)
                .map(|body| body.0.position);
            let (variant, anchor, secondary) = match shot.victim {
                Some((_, victim_position, _)) => (
                    super::spells::CreationVariant::Object,
                    firer_position.unwrap_or(shot.centre),
                    Some(victim_position),
                ),
                None => (super::spells::CreationVariant::At, shot.centre, None),
            };
            let made = self.run_creation_list(
                tick,
                ids,
                shot.attacker,
                &nugget.list,
                anchor,
                variant,
                owner,
                secondary,
                None,
            )?;
            diag!(
                "nugget: WeaponOCLNugget {} at ({:.0},{:.0}) owner {} -> {made} object(s)",
                nugget.list,
                anchor.x,
                anchor.y,
                owner.get()
            );
        }
        for &victim in &result.slave_attacks {
            let ordered = self.order_slaves_to_attack(tick, ids, shot.attacker, victim)?;
            diag!(
                "nugget: SlaveAttackNugget on entity {} -> {ordered} slave(s) at entity {}",
                shot.attacker.index(),
                victim.index()
            );
        }
        for &(victim, index) in &result.conditions {
            let Some(EffectNugget::SpecialModelCondition(nugget)) = effect(index) else {
                continue;
            };
            self.wear_special_conditions(tick, ids, victim, &nugget.conditions, nugget.duration)?;
        }
        for &(victim, index) in &result.emotion_pulses {
            let Some(EffectNugget::EmotionWeapon(nugget)) = effect(index) else {
                continue;
            };
            let Some(kind) = mint_sage_ini::EmotionType::parse(&nugget.emotion) else {
                diag!(
                    "nugget: EmotionWeaponNugget names {}, which is not an emotion type",
                    nugget.emotion
                );
                continue;
            };
            self.force_emotion(
                tick,
                ids,
                victim,
                kind,
                nugget.duration,
                Some(shot.attacker),
            )?;
        }
        for &gate in &result.gates_opened {
            self.open_gate_on_hit(tick, ids, gate)?;
        }
        for &(victim, index) in &result.money_stolen {
            let Some(EffectNugget::StealMoney(nugget)) = effect(index) else {
                continue;
            };
            self.steal_money(tick, ids, shot.attacker, victim, i64::from(nugget.amount))?;
        }
        for &(victim, index) in &result.grabs {
            let Some(EffectNugget::Grab(nugget)) = effect(index) else {
                continue;
            };
            let grabbed = nugget.contain_target_on_effect
                && self.grab_on_hit(
                    tick,
                    ids,
                    shot.attacker,
                    victim,
                    nugget.remove_target_from_other_contain,
                )?;
            let shoved = !grabbed
                && nugget.impact_target_on_effect
                && self.shove(tick, ids, shot, victim, &nugget.shove())?;
            diag!(
                "nugget: GrabNugget on weapon {} by entity {} on entity {} -> {}",
                weapon.index(),
                shot.attacker.index(),
                victim.index(),
                if grabbed {
                    "grabbed"
                } else if shoved {
                    "shoved"
                } else {
                    "nothing"
                }
            );
        }
        for &index in &result.fire_lightings {
            let Some(EffectNugget::FireLogic(nugget)) = effect(index) else {
                continue;
            };
            let damage_arc = nugget
                .damage_arc
                .unwrap_or(super::terrain_fire::DEFAULT_DAMAGE_ARC);
            let Some(logic_type) =
                super::terrain_fire::FireLogicKind::from_keyword(&nugget.logic_type, damage_arc)
            else {
                diag!(
                    "nugget: FireLogicNugget names {}, which is not a modelled LogicType",
                    nugget.logic_type
                );
                continue;
            };
            let direction = tick.state().get(ids.body, shot.attacker).map(|body| {
                let dir = body.0.facing.to_vector();
                (dir.x, dir.y)
            });
            if let Some(fire_entity) =
                super::terrain_fire::singleton_of(tick.state(), ids.fire_grid)
            {
                if let Some(grid) = tick.state_mut().get_mut(ids.fire_grid, fire_entity) {
                    grid.light(
                        &super::terrain_fire::FireLighting {
                            logic_type,
                            radius: nugget.radius,
                            damage: nugget.damage,
                            damage_arc,
                            min_max_burn_rate: nugget.min_max_burn_rate,
                            min_decay: nugget.min_decay,
                            max_resistance: nugget.max_resistance,
                        },
                        (shot.centre.x, shot.centre.y),
                        direction,
                        &|x, y| self.passability.underwater(Vec2::new(x, y)),
                    );
                    diag!(
                        "fire: FireLogicNugget {} lit at ({:.0},{:.0}), {} cell(s) now burning",
                        nugget.logic_type,
                        shot.centre.x,
                        shot.centre.y,
                        grid.burning_count()
                    );
                }
            }
        }
        for &index in &result.unmodelled {
            let kind = self
                .combat
                .weapons
                .get(weapon)
                .and_then(|template| template.nuggets.get(index))
                .map_or("", mint_sage_combat::Nugget::keyword);
            diag!(
                "nugget: {kind} on weapon {} runs nothing yet",
                weapon.index()
            );
        }
        Ok(())
    }

    fn steal_money(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        thief: Entity,
        victim: Entity,
        amount: i64,
    ) -> Result<()> {
        let economy = Self::economy_ids(ids);
        let owner = |entity: Entity| tick.state().get(ids.owner, entity).map(|o| o.0);
        let (Some(taker), Some(loser)) = (owner(thief), owner(victim)) else {
            return Ok(());
        };
        if taker == loser || amount <= 0 {
            return Ok(());
        }
        let (Some(taker_entity), Some(loser_entity)) = (
            crate::economy::player_entity(tick.state(), economy, taker),
            crate::economy::player_entity(tick.state(), economy, loser),
        ) else {
            return Ok(());
        };
        let mut robbed = crate::economy::purse_of(tick.state(), economy, loser);
        let taken = amount.min(robbed.amount());
        if taken <= 0 {
            return Ok(());
        }
        robbed.withdraw(taken);
        let mut gained = crate::economy::purse_of(tick.state(), economy, taker);
        gained.deposit(taken);
        tick.state_mut()
            .insert(ids.purse, loser_entity, crate::economy::Purse(robbed))?;
        tick.state_mut()
            .insert(ids.purse, taker_entity, crate::economy::Purse(gained))?;
        diag!("nugget: StealMoneyNugget takes {taken} from {loser} for {taker}");
        Ok(())
    }

    pub(super) fn list_end(&self, now: Frame, id: mint_sage_combat::ModifierId) -> Frame {
        self.combat
            .modifiers
            .get(id)
            .and_then(|list| list.duration_ms)
            .filter(|ms| *ms > 0)
            .map_or(Frame::FOREVER, |ms| {
                now.checked_add(mint_sage_modules::retail_duration_ticks(ms))
                    .unwrap_or(Frame::FOREVER)
            })
    }

    fn pulse_dots(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.dots)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, dots)| dots.0.iter().any(|dot| dot.next <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for victim in due {
            let Some(mut dots) = tick.state().get(ids.dots, victim).cloned() else {
                continue;
            };
            let alive = tick
                .state()
                .get(ids.health, victim)
                .is_some_and(|health| !health.0.is_dead());
            let mut pulses: Vec<crate::spell_state::Dot> = Vec::new();
            let left = dots.pulse(now, |dot| pulses.push(dot));
            if alive {
                events.extend(pulses.into_iter().map(|dot| dot.pulse_event(victim)));
            }
            if left && alive {
                tick.state_mut().insert(ids.dots, victim, dots)?;
            } else {
                tick.state_mut().remove(ids.dots, victim);
            }
        }
        Ok(())
    }
}
