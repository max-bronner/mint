use mint_movement::Vec2;
use mint_sage_modules::effects::names;
use mint_sage_modules::{CastEdge, Effect, EffectLog};
use mint_sim::module::OwnerChange;
use mint_sim::{Component, Effects, Entity, Frame, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::shroud::Vision;
use crate::unit::Owner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureBonus {
    pub(crate) amount: i64,
    pub(crate) armed_from: Frame,
    pub(crate) spent: bool,
}

impl Component for CaptureBonus {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i64(self.amount);
        hasher.write_u64(self.armed_from.get());
        hasher.write_bool(self.spent);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CaptureLinks(pub(crate) Vec<Entity>);

impl Component for CaptureLinks {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for entity in &self.0 {
            entity.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Capturer(pub(crate) Entity);

impl Component for Capturer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DefectRevert {
    pub(crate) to: PlayerId,
    pub(crate) by: Entity,
    pub(crate) since: Frame,
    pub(crate) until: Frame,
}

impl Component for DefectRevert {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.to.get());
        self.by.hash_into(hasher);
        hasher.write_u64(self.since.get());
        hasher.write_u64(self.until.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureFlashEvent {
    pub(crate) target: Entity,
    pub(crate) capturer: PlayerId,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CaptureFlashes {
    frame: Frame,
    events: Vec<CaptureFlashEvent>,
}

impl CaptureFlashes {
    fn push(&mut self, now: Frame, event: CaptureFlashEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    fn of(&self, now: Frame) -> &[CaptureFlashEvent] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for CaptureFlashes {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            event.target.hash_into(hasher);
            hasher.write_u8(event.capturer.get());
        }
    }
}

const CAPTUREFLAG: &str = "CAPTUREFLAG";
const CAPTURABLE: &str = "CAPTURABLE";
const LINKED_TO_FLAG: &str = "LINKED_TO_FLAG";
const START_CAPTURE: &str = "START_CAPTURE";
const CANCEL_CAPTURE: &str = "CANCEL_CAPTURE";
const CAPTURED: &str = "CAPTURED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Transfer {
    Capture,
    NamedSetTeam,
    TeamController,
    Prototype,
}

impl RotwkRules {
    pub(super) fn has_kind_bit(&self, world: &World, ids: Ids, entity: Entity, bit: &str) -> bool {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry
                    .record
                    .kind_of
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(bit))
            })
    }

    pub(super) fn kind_facts(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> crate::templates::KindFacts {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| entry.facts)
            .unwrap_or_default()
    }

    pub(super) fn power_captures(&self, power: &str) -> bool {
        self.power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .is_some_and(|rule| rule.captures)
    }

    pub(super) fn link_capture_flags(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let placed: Vec<(Entity, Vec2)> = world
            .store(ids.kind)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entity| Some((entity, world.get(ids.body, entity)?.0.position)))
            .collect();
        let flags: Vec<(Entity, Vec2)> = placed
            .iter()
            .copied()
            .filter(|(entity, _)| self.has_kind_bit(world, ids, *entity, CAPTUREFLAG))
            .collect();
        let linked: Vec<(Entity, Vec2)> = placed
            .iter()
            .copied()
            .filter(|(entity, _)| self.has_kind_bit(world, ids, *entity, LINKED_TO_FLAG))
            .collect();
        if flags.is_empty() && linked.is_empty() {
            return Ok(());
        }
        let mut bound: Vec<(Entity, Vec<Entity>)> =
            flags.iter().map(|(flag, _)| (*flag, Vec::new())).collect();
        let mut unbound_structures = 0usize;
        for &(structure, at) in &linked {
            let nearest = bound
                .iter_mut()
                .zip(&flags)
                .map(|(slot, (_, flag_at))| (slot, flag_at.minus(at).length_squared()))
                .reduce(|best, next| if next.1 < best.1 { next } else { best });
            match nearest {
                Some((slot, _)) => {
                    slot.1.push(structure);
                    tick.state_mut().insert(
                        ids.capture_links,
                        structure,
                        CaptureLinks(vec![slot.0]),
                    )?;
                }
                None => unbound_structures = unbound_structures.saturating_add(1),
            }
        }
        let (mut twice, mut alone) = (0usize, 0usize);
        for (flag, structures) in bound {
            match structures.len() {
                0 => alone = alone.saturating_add(1),
                1 => {}
                _ => twice = twice.saturating_add(1),
            }
            if !structures.is_empty() {
                tick.state_mut()
                    .insert(ids.capture_links, flag, CaptureLinks(structures))?;
            }
        }
        diag!(
            "capture: {} flag(s), {} linked structure(s); {twice} flag(s) bound twice, {alone} \
             flag(s) and {unbound_structures} structure(s) unbound",
            flags.len(),
            linked.len()
        );
        Ok(())
    }

    pub(super) fn attach_capture_bonus(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::unit::Kind,
    ) -> Result<()> {
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return Ok(());
        };
        let (Some((timing, _)), Some(amount)) = (
            crate::templates::income(record),
            record
                .initial_capture_bonus
                .and_then(crate::templates::whole)
                .filter(|amount| *amount > 0),
        ) else {
            return Ok(());
        };
        let armed_from = tick.frame().checked_add(timing).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.capture_bonus,
            entity,
            CaptureBonus {
                amount,
                armed_from,
                spent: false,
            },
        )?;
        Ok(())
    }

    pub(super) fn change_owner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
    ) -> Result<bool> {
        self.change_owner_for(tick, ids, entity, new, OwnerChange::Capture)
    }

    pub(super) fn change_owner_for(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
        reason: OwnerChange,
    ) -> Result<bool> {
        self.transfer_owner(tick, ids, entity, new, reason, Transfer::Capture)
    }

    pub(super) fn change_owner_scripted(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
        transfer: Transfer,
    ) -> Result<bool> {
        self.transfer_owner(tick, ids, entity, new, OwnerChange::Capture, transfer)
    }

    fn transfer_owner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
        reason: OwnerChange,
        transfer: Transfer,
    ) -> Result<bool> {
        if transfer == Transfer::Prototype {
            return self.follow_controller(tick, ids, entity, new);
        }
        if !self.change_one_owner(tick, ids, entity, new, reason, transfer)? {
            return Ok(false);
        }
        if self.has_kind_bit(tick.state(), ids, entity, CAPTUREFLAG) {
            let linked = tick
                .state()
                .get(ids.capture_links, entity)
                .map(|links| links.0.clone())
                .unwrap_or_default();
            for structure in linked {
                self.change_one_owner(tick, ids, structure, new, reason, transfer)?;
            }
        }
        for member in self.castle_members_of(tick.state(), ids, entity) {
            self.change_one_owner(tick, ids, member, new, reason, transfer)?;
        }
        Ok(true)
    }

    fn follow_controller(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
    ) -> Result<bool> {
        let world = tick.state();
        if !world.is_live(entity)
            || world
                .get(ids.owner, entity)
                .is_none_or(|owner| owner.0 == new)
        {
            return Ok(false);
        }
        let mut moved = vec![entity];
        if let Some(horde) = world.get(ids.horde, entity) {
            moved.extend(
                horde
                    .members
                    .iter()
                    .copied()
                    .filter(|member| world.is_live(*member)),
            );
        }
        for each in moved {
            tick.state_mut().insert(ids.owner, each, Owner(new))?;
        }
        Ok(true)
    }

    fn change_one_owner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
        reason: OwnerChange,
        transfer: Transfer,
    ) -> Result<bool> {
        let world = tick.state();
        let Some(old) = world.get(ids.owner, entity).map(|owner| owner.0) else {
            return Ok(false);
        };
        if old == new
            || !world.is_live(entity)
            || (world.contains(ids.doomed, entity) && !self.is_rubble(world, ids, entity))
            || world.contains(ids.construction, entity)
        {
            return Ok(false);
        }
        let mut moved = vec![entity];
        if let Some(horde) = world.get(ids.horde, entity) {
            moved.extend(
                horde
                    .members
                    .iter()
                    .copied()
                    .filter(|member| world.is_live(*member)),
            );
        }
        self.release_slave(tick, entity);
        self.propagate_defect_to_passengers(tick, ids, entity, new, reason, transfer)?;
        let refunded = if transfer == Transfer::Capture {
            self.cancel_all_production(tick, ids, entity, old)?
        } else {
            0
        };
        let before: Vec<mint_sim::UpgradeMask> = moved
            .iter()
            .map(|&each| self.effective_upgrades(tick, ids, each))
            .collect();
        for &each in &moved {
            tick.state_mut().insert(ids.owner, each, Owner(new))?;
        }
        let bonus = if transfer == Transfer::TeamController {
            0
        } else {
            self.award_capture_bonus(tick, ids, entity, new)?
        };
        let now = tick.frame();
        for (&each, before) in moved.iter().zip(before) {
            self.idle_after_capture(tick, ids, each, new)?;
            if let Some(wiring) = self
                .modules
                .as_ref()
                .filter(|_| transfer != Transfer::TeamController)
            {
                let mut log = EffectLog::new();
                wiring.broadcast_capture(
                    tick.state_mut(),
                    each,
                    (old, new),
                    reason,
                    now,
                    Effects::into_sink(&mut log),
                );
                self.apply_effects(tick, ids, log.drain())?;
                let mut removed = before;
                removed.clear_all(&self.effective_upgrades(tick, ids, each));
                if !removed.is_empty() {
                    let mut log = EffectLog::new();
                    wiring.broadcast_upgrade_removed(
                        tick.state_mut(),
                        each,
                        &removed,
                        Effects::into_sink(&mut log),
                    );
                    self.apply_effects(tick, ids, log.drain())?;
                    self.cancel_heroes_missing_upgrade(tick, ids, old)?;
                }
            }
            self.offer_upgrades(tick, ids, each)?;
        }
        let command_set = self.modules.as_ref().and_then(|wiring| {
            tick.state()
                .get(wiring.state, entity)
                .and_then(|state| state.command_set.as_deref().map(str::to_owned))
        });
        let name = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map_or("an object", |entry| entry.record.name.as_str());
        diag!(
            "capture: entity {} ({name}) player {} -> player {}{}{}{}",
            entity.index(),
            old.get(),
            new.get(),
            command_set.map_or_else(String::new, |set| format!(", command set {set}")),
            if refunded > 0 {
                format!(", {refunded} refunded to player {}", old.get())
            } else {
                String::new()
            },
            if bonus > 0 {
                format!(", capture bonus {bonus} paid")
            } else {
                String::new()
            }
        );
        Ok(true)
    }

    pub(super) fn release_slave(&self, tick: &mut Tick<'_>, entity: Entity) {
        if let Some(wiring) = self.modules.as_ref() {
            tick.state_mut().remove(wiring.slaved_to, entity);
        }
    }

    fn propagate_defect_to_passengers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        new: PlayerId,
        reason: OwnerChange,
        transfer: Transfer,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        if wiring.is_garrison(tick.state(), container) {
            return Ok(());
        }
        let Some(passengers) = tick
            .state()
            .get(ids.passengers, container)
            .map(|list| list.0.clone())
        else {
            return Ok(());
        };
        if passengers.is_empty() {
            return Ok(());
        }
        let passenger_reason = if wiring.siege_contain(tick.state(), container).is_some() {
            reason
        } else {
            OwnerChange::Defect
        };
        for passenger in passengers {
            self.change_one_owner(tick, ids, passenger, new, passenger_reason, transfer)?;
        }
        Ok(())
    }

    pub(super) fn expire_defects(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let world = tick.state();
        let due: Vec<(Entity, PlayerId)> = world
            .store(ids.defect_revert)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, revert)| {
                        revert.until <= now || Self::dominator_is_gone(world, ids, revert.by)
                    })
                    .map(|(entity, revert)| (entity, revert.to))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, player) in due {
            tick.state_mut().remove(ids.defect_revert, entity);
            if self.change_owner(tick, ids, entity, player)? {
                diag!(
                    "capture: entity {} returns to player {} as its defect ends",
                    entity.index(),
                    player.get()
                );
            }
        }
        Ok(())
    }

    fn dominator_is_gone(world: &World, ids: Ids, by: Entity) -> bool {
        !world.is_live(by)
            || world.contains(ids.doomed, by)
            || world
                .get(ids.health, by)
                .is_some_and(|health| health.0.is_dead())
    }

    pub(super) fn set_defect_dominator(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        by: Entity,
        was_defecting: bool,
    ) -> Result<()> {
        if was_defecting {
            return Ok(());
        }
        let Some(mut revert) = tick.state().get(ids.defect_revert, entity).copied() else {
            return Ok(());
        };
        revert.by = by;
        tick.state_mut().insert(ids.defect_revert, entity, revert)?;
        Ok(())
    }

    fn award_capture_bonus(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
    ) -> Result<i64> {
        let economy = Self::economy_ids(ids);
        let Some(player) = crate::economy::player_entity(tick.state(), economy, new) else {
            return Ok(0);
        };
        let timing = match tick.state_mut().get_mut(ids.income, entity) {
            Some(income) => {
                income.0.restart();
                income.0.timing()
            }
            None => return Ok(0),
        };
        let now = tick.frame();
        let Some(mut bonus) = tick.state().get(ids.capture_bonus, entity).copied() else {
            return Ok(0);
        };
        if bonus.spent {
            return Ok(0);
        }
        if now < bonus.armed_from {
            bonus.armed_from = now.checked_add(timing).unwrap_or(Frame::FOREVER);
            tick.state_mut().insert(ids.capture_bonus, entity, bonus)?;
            return Ok(0);
        }
        bonus.spent = true;
        tick.state_mut().insert(ids.capture_bonus, entity, bonus)?;
        let mut purse = crate::economy::purse_of(tick.state(), economy, new);
        purse.deposit(bonus.amount);
        tick.state_mut()
            .insert(ids.purse, player, crate::economy::Purse(purse))?;
        Ok(bonus.amount)
    }

    fn idle_after_capture(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        new: PlayerId,
    ) -> Result<()> {
        let world = tick.state_mut();
        world.remove(ids.selected, entity);
        world.remove(ids.attack_order, entity);
        world.remove(ids.attack_move_order, entity);
        world.remove(ids.ground_order, entity);
        world.remove(ids.horde_target, entity);
        world.remove(ids.player_order, entity);
        world.remove(ids.path, entity);
        if new.get() >= self.players {
            tick.state_mut().remove(ids.vision, entity);
            return Ok(());
        }
        let world = tick.state();
        if world.contains(ids.vision, entity) || world.contains(ids.member, entity) {
            return Ok(());
        }
        let range = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| crate::templates::vision(&entry.record));
        if let Some(clearing_range) = range {
            tick.state_mut()
                .insert(ids.vision, entity, Vision { clearing_range })?;
        }
        Ok(())
    }

    pub(crate) fn capture_target(
        &self,
        world: &World,
        player: PlayerId,
        target: Entity,
    ) -> std::result::Result<Entity, &'static str> {
        let Some(ids) = self.ids else {
            return Err("no match");
        };
        if !world.is_live(target) || world.contains(ids.doomed, target) {
            return Err("its flag is gone");
        }
        if !self.has_kind_bit(world, ids, target, CAPTURABLE) {
            return Err("the target is not CAPTURABLE");
        }
        if world.contains(ids.owner, target)
            && self.view_relation(world, player, target) == super::map_scripts::Relation::Allies
        {
            return Err("its flag is already held by the capturer's side");
        }
        Ok(target)
    }

    pub(crate) fn contests(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> bool {
        if !world.is_live(target) || world.contains(ids.doomed, target) {
            return false;
        }
        if !self.has_kind_bit(world, ids, target, CAPTURABLE) {
            return false;
        }
        world.contains(ids.owner, target)
            && self.view_relation(world, player, target) != super::map_scripts::Relation::Allies
    }

    pub(crate) fn selection_captures(&self, world: &World, player: PlayerId) -> bool {
        let Some(ids) = self.ids else {
            return false;
        };
        self.command_subjects_in(world, ids, player)
            .into_iter()
            .any(|subject| self.capture_power_of(world, ids, subject).is_some())
    }

    pub(crate) fn selection_capture_power(
        &self,
        world: &World,
        player: PlayerId,
        target: Entity,
    ) -> Option<String> {
        let ids = self.ids?;
        self.capture_target(world, player, target).ok()?;
        self.command_subjects_in(world, ids, player)
            .into_iter()
            .find_map(|subject| self.capture_power_of(world, ids, subject))
    }

    fn capture_power_of(&self, world: &World, ids: Ids, entity: Entity) -> Option<String> {
        let entry = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))?;
        entry
            .record
            .powers
            .iter()
            .find(|slot| self.power_captures(&slot.power))
            .map(|slot| slot.power.clone())
    }

    pub(super) fn capture_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> Result<()> {
        for subject in self.command_subjects_in(tick.state(), ids, player) {
            let owned = tick
                .state()
                .get(ids.owner, subject)
                .is_some_and(|owner| owner.0 == player);
            if !owned || tick.state().contains(ids.no_attack, subject) {
                continue;
            }
            let Some(power) = self.capture_power_of(tick.state(), ids, subject) else {
                continue;
            };
            self.fire_power_from(
                tick,
                ids,
                player,
                subject,
                &power,
                crate::templates::TargetKind::Object,
                &crate::defeat::ObjectFilter::default(),
                Some(target),
                None,
                false,
            )?;
        }
        Ok(())
    }

    pub(super) fn cast_edge(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
        target: Option<Entity>,
        edge: CastEdge,
    ) -> Result<()> {
        if !self.power_captures(power) {
            return Ok(());
        }
        let (Some(target), Some(player)) = (
            target,
            tick.state().get(ids.owner, source).map(|owner| owner.0),
        ) else {
            return Ok(());
        };
        let frame = tick.frame().get();
        let flag = match self.capture_target(tick.state(), player, target) {
            Ok(flag) => flag,
            Err(reason) => {
                if edge == CastEdge::Trigger {
                    diag!(
                        "capture: frame {frame} entity {} completes, but not for player {} - \
                         {reason}",
                        source.index(),
                        player.get()
                    );
                }
                return Ok(());
            }
        };
        match edge {
            CastEdge::Prepare => {
                diag!(
                    "capture: frame {frame} entity {} starts capturing flag entity {} for \
                     player {}",
                    source.index(),
                    flag.index(),
                    player.get()
                );
                tick.state_mut()
                    .insert(ids.capturer, flag, Capturer(source))?;
                self.set_conditions(tick, ids, flag, &[START_CAPTURE], &[CANCEL_CAPTURE])
            }
            CastEdge::Abort => {
                diag!(
                    "capture: frame {frame} entity {} stops capturing flag entity {}",
                    source.index(),
                    flag.index()
                );
                tick.state_mut().remove(ids.capturer, flag);
                self.set_conditions(tick, ids, flag, &[CANCEL_CAPTURE], &[START_CAPTURE])
            }
            CastEdge::Trigger => {
                diag!(
                    "capture: frame {frame} entity {} completes the capture of flag entity {}",
                    source.index(),
                    flag.index()
                );
                if !self.change_owner(tick, ids, flag, player)? {
                    return Ok(());
                }
                tick.state_mut().remove(ids.capturer, flag);
                let mut taken = vec![flag];
                taken.extend(
                    tick.state()
                        .get(ids.capture_links, flag)
                        .map(|links| links.0.clone())
                        .unwrap_or_default(),
                );
                for each in taken {
                    self.set_conditions(
                        tick,
                        ids,
                        each,
                        &[CAPTURED],
                        &[START_CAPTURE, CANCEL_CAPTURE],
                    )?;
                }
                Ok(())
            }
        }
    }

    pub(crate) fn capture_flashes<'w>(
        &self,
        world: &'w World,
        frame: Frame,
    ) -> &'w [CaptureFlashEvent] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.capture_flashes)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, flashes)| flashes.of(frame))
    }

    pub(super) fn apply_capture_flash(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        caster: Entity,
        target: Entity,
    ) -> Result<()> {
        let Some(capturer) = tick.state().get(ids.owner, caster).map(|owner| owner.0) else {
            return Ok(());
        };
        let at = tick
            .state()
            .get(ids.body, target)
            .map_or(Vec2::ZERO, |body| body.0.position);
        let now = tick.frame();
        if let Some(store) = tick.state_mut().store_mut(ids.capture_flashes) {
            if let Some((_, flashes)) = store.iter_mut().next() {
                flashes.push(now, CaptureFlashEvent { target, capturer });
            }
        }
        self.push_misc_sound(tick, ids, super::MiscSound::DefectorTimerTick, capturer, at);
        Ok(())
    }

    pub(super) fn set_conditions(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        add: &[&str],
        remove: &[&str],
    ) -> Result<()> {
        let list = |flags: &[&str]| names(flags.iter().map(|flag| (*flag).to_owned()).collect());
        self.apply_effects(
            tick,
            ids,
            vec![(
                entity,
                Effect::ModelConditions {
                    add: list(add),
                    remove: list(remove),
                },
            )],
        )
    }
}
