use mint_movement::{Facing, Vec2};
use mint_sage_modules::effects::{names, CastEdge, Effect, Name};
use mint_sim::module::ModuleKind;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{DamageEvent, DeathNaming, Foe};
use crate::diag::diag;
use crate::fx::FxSource;
use crate::powers::GrabEnum;
use crate::templates::TargetKind;

const SCAN_INTERVAL: LogicFrames = LogicFrames::from_frames(15);

const GRAB_AND_KILL: &str = "GRAB_AND_KILL";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PickUpOrder {
    target: Entity,
    eats: bool,
}

impl Component for PickUpOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.target.hash_into(hasher);
        hasher.write_bool(self.eats);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Eating {
    passenger: Entity,
    state: Name,
    starts: Frame,
    consumes: Frame,
    ends: Frame,
    raised: bool,
    consumed: bool,
}

impl Component for Eating {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.passenger.hash_into(hasher);
        hasher.write_usize(self.state.len());
        hasher.write_bytes(self.state.as_bytes());
        self.starts.hash_into(hasher);
        self.consumes.hash_into(hasher);
        self.ends.hash_into(hasher);
        hasher.write_bool(self.raised);
        hasher.write_bool(self.consumed);
    }
}

#[derive(Clone, Copy, Default)]
struct Scan {
    fired: bool,
    spawned: bool,
}

impl RotwkRules {
    fn power_grab(&self, power: &str) -> Option<GrabEnum> {
        self.power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .and_then(|rule| rule.grab)
    }

    pub(crate) fn map_tree_at(world: &World, ids: Ids, at: Vec2) -> Option<(Vec2, bool)> {
        let forest = world.get(ids.forest, crate::forest::singleton_of(world, ids.forest)?)?;
        let handle = forest.find_closest(at, 10.0, false, crate::forest::TreeQuery::Any)?;
        forest.tree(handle).map(|tree| (tree.pos, tree.grabbable))
    }

    pub(crate) fn grabs_map_tree(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        power: &str,
        at: (f32, f32),
    ) -> bool {
        if self.power_grab(power).is_none() {
            return false;
        }
        let allows_tree = world
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| {
                entry.record.declared_modules.iter().find(|module| {
                    module
                        .name
                        .eq_ignore_ascii_case("GrabPassengerSpecialPower")
                })
            })
            .and_then(|module| module.field("AllowTree"))
            .and_then(|field| field.args.first())
            .and_then(|value| mint_sage_ini::value::parse_bool(value))
            .unwrap_or(false);
        allows_tree
            && Self::map_tree_at(world, ids, Vec2::new(at.0, at.1))
                .is_some_and(|(_, grabbable)| grabbable)
    }

    pub(super) fn manual_pick_up_refusal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        grabber: Entity,
        power: &str,
        target: Option<Entity>,
    ) -> Option<String> {
        let target = target?;
        let grab = self.power_grab(power)?;
        let world = tick.state();
        let record = world
            .get(ids.kind, target)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)?;
        let filter = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.transport_view(world, grabber))
            .and_then(|contain| contain.manual_pick_up_filter());
        if filter.is_some_and(|filter| !filter.passes(&record.kind_of, &record.name)) {
            return Some(format!(
                "its ManualPickUpFilter refuses {} (entity {})",
                record.name,
                target.index()
            ));
        }
        if record.has_kind(grab.vendor_kind()) {
            return None;
        }
        let owner = world.get(ids.owner, target)?.0;
        self.contain_admits(world, ids, owner, grabber, target)
            .err()
            .map(|refusal| format!("its contain refuses entity {}: {refusal:?}", target.index()))
    }

    pub(super) fn pick_up_edge(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        power: &str,
        target: Option<Entity>,
        edge: CastEdge,
    ) -> Result<()> {
        let Some(grab) = self.power_grab(power) else {
            return Ok(());
        };
        let target = target.filter(|target| tick.state().is_live(*target));
        match edge {
            CastEdge::Prepare | CastEdge::Abort => Ok(()),
            CastEdge::Trigger => self.grab_trigger(tick, ids, grabber, power, grab, target),
            CastEdge::Finish { triggers } => {
                self.grab_finish(tick, ids, grabber, power, target, triggers)
            }
        }
    }

    fn grab_trigger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        power: &str,
        grab: GrabEnum,
        target: Option<Entity>,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let cast = tick
            .state()
            .get(wiring.ability_state, grabber)
            .and_then(|column| {
                column
                    .0
                    .iter()
                    .find(|slot| slot.state.is_for_power(power))
                    .map(|slot| {
                        (
                            slot.state.triggers(),
                            slot.state.target_position(),
                            slot.state.packing_type().cloned(),
                        )
                    })
            });
        let triggers = cast.as_ref().map_or(1, |cast| cast.0);
        let mut target = target;
        if target.is_none() && triggers == 1 {
            if let Some(at) = cast.as_ref().map(|cast| cast.1.unwrap_or((0.0, 0.0))) {
                target = self.adopt_tree(tick, ids, Vec2::new(at.0, at.1))?;
                if let Some(tree) = target {
                    if let Some(column) = tick.state_mut().get_mut(wiring.ability_state, grabber) {
                        for slot in column.0.iter_mut().filter(|s| s.state.is_for_power(power)) {
                            slot.state.retarget(tree);
                        }
                    }
                    diag!(
                        "contain: frame {} entity {} adopts the map tree at ({:.1},{:.1}) as \
                         entity {}",
                        tick.frame().get(),
                        grabber.index(),
                        at.0,
                        at.1,
                        tree.index()
                    );
                }
            }
        }
        let shrubbery = target.is_some_and(|target| self.has_kind(tick, ids, target, "SHRUBBERY"));
        if grab == GrabEnum::Passenger {
            if let Some(column) = tick.state_mut().get_mut(wiring.ability_state, grabber) {
                for slot in column.0.iter_mut().filter(|s| s.state.is_for_power(power)) {
                    match (triggers, target) {
                        (1, None) => slot.state.cut_period(),
                        (1, Some(_)) if shrubbery => {}
                        _ => slot.state.end_persistence_now(),
                    }
                }
            }
        }
        if triggers == 2 {
            match target {
                Some(target) if shrubbery => self.club(tick, ids, grabber, target)?,
                Some(target) if self.has_kind(tick, ids, target, GRAB_AND_KILL) => {
                    let packing = cast.and_then(|cast| cast.2);
                    self.release_grab_and_kill(tick, ids, grabber, target, packing)?;
                    if let Some(column) = tick.state_mut().get_mut(wiring.ability_state, grabber) {
                        for slot in column.0.iter_mut().filter(|s| s.state.is_for_power(power)) {
                            slot.state.mark_no_target();
                        }
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        let Some(target) = target.filter(|_| triggers == 1) else {
            return Ok(());
        };
        if self.has_kind(tick, ids, target, grab.vendor_kind()) {
            return self.vend(tick, ids, grabber, target);
        }
        let frame = tick.frame().get();
        let dead = tick
            .state()
            .get(ids.health, target)
            .is_some_and(|health| health.0.is_dead());
        if dead || !self.grab_on_hit(tick, ids, grabber, target, false)? {
            diag!(
                "contain: frame {frame} entity {} picks up nothing - entity {} is gone or refused",
                grabber.index(),
                target.index()
            );
            return Ok(());
        }
        diag!(
            "contain: frame {frame} entity {} picked up entity {} with {power}",
            grabber.index(),
            target.index()
        );
        if self.has_kind(tick, ids, target, GRAB_AND_KILL) {
            self.set_conditions(tick, ids, target, &["PREPARING"], &[])?;
        }
        Ok(())
    }

    fn release_grab_and_kill(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        target: Entity,
        packing: Option<Name>,
    ) -> Result<()> {
        let held = tick
            .state()
            .get(ids.contained, target)
            .is_some_and(|contained| contained.container == grabber);
        if held {
            self.take_out(tick, ids, grabber, target)?;
        }
        self.set_conditions(tick, ids, target, &[], &["PREPARING"])?;
        let add: Vec<&str> = packing.as_deref().into_iter().collect();
        self.set_conditions(tick, ids, grabber, &add, &["PREPARING"])?;
        diag!(
            "contain: frame {} entity {} lets GRAB_AND_KILL entity {} go",
            tick.frame().get(),
            grabber.index(),
            target.index()
        );
        Ok(())
    }

    fn adopt_tree(&self, tick: &mut Tick<'_>, ids: Ids, at: Vec2) -> Result<Option<Entity>> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(None);
        };
        let Some(forest) = tick.state().get(ids.forest, forest_entity) else {
            return Ok(None);
        };
        if forest
            .find_closest(at, 5.0, true, crate::forest::TreeQuery::Grabbable)
            .is_none()
        {
            return Ok(None);
        }
        let Some(handle) = forest.find_closest(at, 10.0, false, crate::forest::TreeQuery::Any)
        else {
            return Ok(None);
        };
        self.convert_tree(tick, ids, forest_entity, handle)
    }

    fn club(&self, tick: &mut Tick<'_>, ids: Ids, grabber: Entity, target: Entity) -> Result<()> {
        let alive = tick
            .state()
            .get(ids.health, target)
            .is_some_and(|health| !health.0.is_dead());
        if !alive {
            return Ok(());
        }
        self.set_conditions(tick, ids, target, &["CLUB"], &[])?;
        diag!(
            "contain: frame {} entity {} smashes entity {} into a club",
            tick.frame().get(),
            grabber.index(),
            target.index()
        );
        Ok(())
    }

    fn vend(&self, tick: &mut Tick<'_>, ids: Ids, grabber: Entity, vendor: Entity) -> Result<()> {
        let Some(grab) = tick
            .state()
            .get(ids.kind, vendor)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| entry.record.grab.clone())
        else {
            return Ok(());
        };
        let Some((position, facing)) = tick
            .state()
            .get(ids.body, grabber)
            .map(|body| (body.0.position, body.0.facing))
        else {
            return Ok(());
        };
        let frame = tick.frame().get();
        if let Some(name) = grab.object.as_deref() {
            let made = match (self.kinds.kind(name), self.neutral_player()) {
                (Some(kind), Some(neutral)) => {
                    self.spawn_kind_at(tick, ids, kind, position, Facing::EAST, neutral)?
                }
                _ => None,
            };
            match made {
                Some(object) => {
                    self.take_into(tick, ids, grabber, object)?;
                    diag!(
                        "contain: frame {frame} entity {} takes {name} (entity {}) from vendor \
                         entity {}",
                        grabber.index(),
                        object.index(),
                        vendor.index()
                    );
                }
                None => diag!(
                    "contain: frame {frame} vendor entity {} names GrabObject {name}, which \
                     makes nothing",
                    vendor.index()
                ),
            }
        }
        if let Some(damage_type) = self.combat.damage_types.lookup("STRUCTURAL") {
            let mut events = [DamageEvent {
                victim: vendor,
                attacker: Entity::NONE,
                amount: grab.damage,
                damage_type,
                scalar: 1.0,
                fx_type: None,
                dealt: 0.0,
                death: DeathNaming::of(self.combat.death_types.lookup("NONE")),
                sub_type: None,
                flanking_bonus: 0.0,
                weapon: None,
                launched: None,
                nugget: 0,
                accepts_add: true,
                bonuses_applied: false,
                force_kill: false,
                drain_life: 0.0,
            }];
            let killed = self.apply_damage(tick, ids, &mut events)?;
            self.resolve_deaths(tick, ids, &killed)?;
        }
        if let Some(fx) = grab.fx.as_deref() {
            let at = crate::templates::rotate(position, facing, [grab.offset.0, grab.offset.1]);
            self.emit_fx_named(tick, ids, fx, at, FxSource::Power, grabber);
        }
        Ok(())
    }

    fn grab_finish(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        power: &str,
        target: Option<Entity>,
        triggers: u32,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let order = tick
            .state()
            .get(wiring.pick_up_order, grabber)
            .copied()
            .filter(|order| Some(order.target) == target);
        if order.is_some() {
            tick.state_mut().remove(wiring.pick_up_order, grabber);
        }
        let Some(target) = target.filter(|_| triggers == 1) else {
            return Ok(());
        };
        if self.has_kind(tick, ids, target, "SHRUBBERY") {
            let clubbed = tick
                .state()
                .get(wiring.state, target)
                .is_some_and(|state| state.has_model_condition("CLUB"));
            if !clubbed {
                self.club(tick, ids, grabber, target)?;
            }
        } else if self.has_kind(tick, ids, target, "CHUNK_VENDOR") {
            self.set_conditions(tick, ids, grabber, &[], &["GRAB_BUILDING_CHUNK"])?;
        }
        if !order.is_some_and(|order| order.eats) {
            return Ok(());
        }
        let Some(eat) = self.eat_of(tick, ids, grabber, power) else {
            return Ok(());
        };
        let now = tick.frame();
        let mut gained = 0.0;
        if let Some(health) = tick.state_mut().get_mut(ids.health, grabber) {
            let before = health.0.current();
            let percent = (health.0.fraction() + eat.heal.unwrap_or(1.0)) * 100.0;
            health.0.set_initial_percent(percent.min(100.0));
            gained = health.0.current() - before;
        }
        diag!(
            "contain: frame {} entity {} starts eating entity {} and heals {gained:.0}",
            now.get(),
            grabber.index(),
            target.index()
        );
        let eating = Eating {
            passenger: target,
            state: eat.state,
            starts: now,
            consumes: now.checked_add(eat.trigger).unwrap_or(Frame::FOREVER),
            ends: now.checked_add(eat.lasts).unwrap_or(Frame::FOREVER),
            raised: false,
            consumed: false,
        };
        tick.state_mut().insert(wiring.eating, grabber, eating)?;
        Self::start_ai_busy(tick, ids, grabber, eat.lasts)
    }

    fn has_kind(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, kind: &str) -> bool {
        tick.state()
            .get(ids.kind, entity)
            .and_then(|held| self.kinds.get(*held))
            .is_some_and(|entry| entry.record.has_kind(kind))
    }

    fn eat_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        grabber: Entity,
        power: &str,
    ) -> Option<mint_sage_modules::GrabPassengerEat> {
        let entry = tick
            .state()
            .get(ids.kind, grabber)
            .and_then(|kind| self.kinds.get(*kind))?;
        entry.record.declared_modules.iter().find_map(|module| {
            let names_power = module
                .field("SpecialPowerTemplate")
                .and_then(|field| field.args.first())
                .is_some_and(|named| named.eq_ignore_ascii_case(power));
            if !names_power {
                return None;
            }
            mint_sage_modules::grab_passenger_eat(&module.source(&self.upgrade_bits))
                .ok()
                .flatten()
        })
    }

    pub(super) fn wake_auto_pick_up(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        option: u8,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let owned: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .collect();
        for entity in owned {
            let woken = match tick.state_mut().get_mut(wiring.pick_up_state, entity) {
                Some(column) => column
                    .0
                    .iter_mut()
                    .fold(false, |any, slot| slot.state.wake(option) || any),
                None => false,
            };
            if woken {
                diag!(
                    "order: entity {} wakes AutoPickUpUpdate option {option}",
                    entity.index()
                );
            } else {
                diag!(
                    "order: entity {} refuses WAKE_AUTO_PICKUP: no AutoPickUpUpdate with \
                     RunFromButtonNumber {option}",
                    entity.index()
                );
            }
        }
        Ok(())
    }

    pub(super) fn advance_pick_ups(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<(bool, Vec<Entity>)> {
        let eaten = self.advance_eating(tick, ids)?;
        let spawned = self.scan_pick_ups(tick, ids, living, &eaten)?;
        Ok((spawned, eaten))
    }

    fn advance_eating(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<Vec<Entity>> {
        let mut eaten = Vec::new();
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(eaten);
        };
        let now = tick.frame();
        let eaters: Vec<Entity> = tick
            .state()
            .store(wiring.eating)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for eater in eaters {
            let Some(mut eating) = tick.state().get(wiring.eating, eater).cloned() else {
                continue;
            };
            let eater_alive = tick.state().is_live(eater)
                && !tick.state().contains(ids.doomed, eater)
                && tick
                    .state()
                    .get(ids.health, eater)
                    .is_some_and(|health| !health.0.is_dead());
            if !eater_alive || now >= eating.ends {
                if eating.raised {
                    self.eating_condition(tick, ids, eater, &eating, false)?;
                }
                tick.state_mut().remove(wiring.eating, eater);
                continue;
            }
            if !eating.raised && now >= eating.starts {
                eating.raised = true;
                self.eating_condition(tick, ids, eater, &eating, true)?;
            }
            if !eating.consumed && now >= eating.consumes {
                eating.consumed = true;
                if self.consume(tick, ids, eater, eating.passenger)? {
                    eaten.push(eating.passenger);
                }
            }
            tick.state_mut().insert(wiring.eating, eater, eating)?;
        }
        Ok(eaten)
    }

    fn eating_condition(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        eater: Entity,
        eating: &Eating,
        raise: bool,
    ) -> Result<()> {
        let named = names(vec![eating.state.to_string()]);
        let (add, remove) = if raise {
            (named, names(Vec::new()))
        } else {
            (names(Vec::new()), named)
        };
        let effect = Effect::ModelConditions { add, remove };
        let mut effects = vec![(eater, effect.clone())];
        if tick.state().is_live(eating.passenger) {
            effects.push((eating.passenger, effect));
        }
        self.apply_effects(tick, ids, effects)
    }

    fn consume(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        eater: Entity,
        passenger: Entity,
    ) -> Result<bool> {
        if !tick.state().is_live(passenger) {
            return Ok(false);
        }
        let held = tick
            .state()
            .get(ids.contained, passenger)
            .is_some_and(|contained| contained.container == eater);
        if held {
            self.take_out(tick, ids, eater, passenger)?;
        }
        self.award_kills(tick, ids, &[(passenger, eater)])?;
        diag!(
            "contain: frame {} entity {} eats entity {}",
            tick.frame().get(),
            eater.index(),
            passenger.index()
        );
        Ok(true)
    }

    fn scan_pick_ups(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        eaten: &[Entity],
    ) -> Result<bool> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(false);
        };
        let now = tick.frame();
        let mut spawned = false;
        let mut grabbers: Vec<Entity> = tick
            .state()
            .store(wiring.pick_up_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        grabbers.sort_unstable_by_key(|entity| entity.index());
        for grabber in grabbers {
            let due: Vec<mint_sage_modules::AutoPickUpUpdate> = {
                let Some(column) = tick.state_mut().get_mut(wiring.pick_up_state, grabber) else {
                    continue;
                };
                let mut due = Vec::new();
                for slot in &mut column.0 {
                    if slot.state.due(now) {
                        due.push(slot.state.clone());
                        let wait = slot.state.scan_delay().max(SCAN_INTERVAL);
                        slot.state.next_scan = now.checked_add(wait).unwrap_or(Frame::FOREVER);
                        slot.state.woken = false;
                    }
                }
                due
            };
            for module in due {
                let scan = self.pick_up_once(tick, ids, grabber, &module, living, eaten)?;
                spawned |= scan.spawned;
                if scan.fired {
                    break;
                }
            }
        }
        Ok(spawned)
    }

    fn pick_up_once(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        module: &mint_sage_modules::AutoPickUpUpdate,
        living: &[(Foe, PlayerId)],
        eaten: &[Entity],
    ) -> Result<Scan> {
        let none = Scan::default();
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(none);
        };
        let world = tick.state();
        if world.contains(ids.doomed, grabber) {
            return Ok(none);
        }
        let (Some(owner), Some(entry), Some(health), Some(body)) = (
            world.get(ids.owner, grabber).map(|owner| owner.0),
            world
                .get(ids.kind, grabber)
                .and_then(|kind| self.kinds.get(*kind)),
            world.get(ids.health, grabber).map(|health| health.0),
            world.get(ids.body, grabber).map(|body| body.0.position),
        ) else {
            return Ok(none);
        };
        if health.is_dead() {
            return Ok(none);
        }
        let Some(power) = entry
            .record
            .powers
            .iter()
            .find(|slot| self.power_grab(&slot.power) == Some(GrabEnum::Passenger))
            .map(|slot| slot.power.clone())
        else {
            return Ok(none);
        };
        let eat = module.eat();
        if eat.is_some_and(|eat| health.current() > eat.my_health * health.max()) {
            return Ok(none);
        }
        let reach = module.scan_distance();
        let mut best: Option<(f32, Entity)> = None;
        for (foe, candidate_owner) in living {
            let candidate = foe.entity;
            let distance = distance_squared(body, foe.position);
            if candidate == grabber
                || distance > reach * reach
                || best.is_some_and(|(nearest, _)| distance >= nearest)
                || world.contains(ids.contained, candidate)
                || world.contains(ids.doomed, candidate)
                || eaten.contains(&candidate)
            {
                continue;
            }
            let Some(record) = world
                .get(ids.kind, candidate)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
            else {
                continue;
            };
            if module
                .pick_up_filter()
                .is_some_and(|filter| !filter.passes(&record.kind_of, &record.name))
            {
                continue;
            }
            if let Some(eat) = eat {
                let fed = world.get(ids.health, candidate).is_some_and(|held| {
                    held.0.current() <= eat.target_health * held.0.max()
                        && eat.filter.passes(&record.kind_of, &record.name)
                });
                if !fed {
                    continue;
                }
            }
            if self
                .contain_admits(world, ids, *candidate_owner, grabber, candidate)
                .is_err()
            {
                continue;
            }
            best = Some((distance, candidate));
        }
        let mut spawned = false;
        let target = match best {
            Some((_, target)) => target,
            None if eat.is_none()
                && module
                    .pick_up_filter()
                    .is_some_and(|filter| filter.passes(&[String::from("TREE")], "")) =>
            {
                match self.convert_grabbable_tree(tick, ids, body, reach)? {
                    Some(target) => {
                        spawned = true;
                        target
                    }
                    None => return Ok(none),
                }
            }
            None => return Ok(none),
        };
        if !self.fire_power_from(
            tick,
            ids,
            owner,
            grabber,
            &power,
            TargetKind::Object,
            &crate::defeat::ObjectFilter::default(),
            Some(target),
            None,
            false,
        )? {
            return Ok(Scan {
                fired: false,
                spawned,
            });
        }
        diag!(
            "power: {power} at entity {} auto pick-up of entity {}{}",
            grabber.index(),
            target.index(),
            if eat.is_some() { " to eat" } else { "" }
        );
        tick.state_mut().insert(
            wiring.pick_up_order,
            grabber,
            PickUpOrder {
                target,
                eats: eat.is_some(),
            },
        )?;
        Ok(Scan {
            fired: true,
            spawned,
        })
    }

    fn convert_grabbable_tree(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        pos: Vec2,
        reach: f32,
    ) -> Result<Option<Entity>> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(None);
        };
        let handle = tick
            .state()
            .get(ids.forest, forest_entity)
            .and_then(|forest| {
                forest.find_closest(pos, reach, true, crate::forest::TreeQuery::Grabbable)
            });
        let Some(handle) = handle else {
            return Ok(None);
        };
        self.convert_tree(tick, ids, forest_entity, handle)
    }

    fn convert_tree(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        forest_entity: Entity,
        handle: crate::forest::TreeHandle,
    ) -> Result<Option<Entity>> {
        if self.neutral_player().is_none() {
            return Ok(None);
        }
        let tree = tick
            .state_mut()
            .get_mut(ids.forest, forest_entity)
            .and_then(|forest| forest.take_tree(handle));
        let Some(tree) = tree else {
            return Ok(None);
        };
        let Some(neutral) = self.neutral_player() else {
            return Ok(None);
        };
        self.spawn_kind_at(
            tick,
            ids,
            tree.kind,
            tree.pos,
            mint_movement::Facing::EAST,
            neutral,
        )
    }
}

fn distance_squared(a: Vec2, b: Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}
