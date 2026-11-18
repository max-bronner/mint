use mint_movement::Vec2;
use mint_sage_modules::effects::{names, Effect, Name};
use mint_sim::module::ModuleKind;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::combat::Foe;
use crate::diag::diag;
use crate::templates::TargetKind;

const SCAN_INTERVAL: LogicFrames = LogicFrames::from_frames(15);

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
    heal: f32,
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
        hasher.write_f32(self.heal);
        hasher.write_bool(self.raised);
        hasher.write_bool(self.consumed);
    }
}

impl RotwkRules {
    fn power_grabs_passenger(&self, power: &str) -> bool {
        self.power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .is_some_and(|rule| rule.grabs_passenger)
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
        if !self.power_grabs_passenger(power) {
            return None;
        }
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
        edge: mint_sage_modules::effects::CastEdge,
    ) -> Result<()> {
        use mint_sage_modules::effects::CastEdge;
        let (Some(wiring), Some(target)) = (self.modules.as_ref(), target) else {
            return Ok(());
        };
        if edge == CastEdge::Prepare || !self.power_grabs_passenger(power) {
            return Ok(());
        }
        let order = tick
            .state()
            .get(wiring.pick_up_order, grabber)
            .copied()
            .filter(|order| order.target == target);
        if order.is_some() {
            tick.state_mut().remove(wiring.pick_up_order, grabber);
        }
        if edge == CastEdge::Abort {
            return Ok(());
        }
        if let Some(column) = tick.state_mut().get_mut(wiring.ability_state, grabber) {
            for slot in &mut column.0 {
                if slot.state.is_for_power(power) {
                    slot.state.end_persistence();
                }
            }
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
        if !order.is_some_and(|order| order.eats) {
            return Ok(());
        }
        let Some(eat) = self.eat_of(tick, ids, grabber, power) else {
            return Ok(());
        };
        let now = tick.frame();
        let starts = now.checked_add(eat.after).unwrap_or(Frame::FOREVER);
        let eating = Eating {
            passenger: target,
            state: eat.state,
            starts,
            consumes: starts.checked_add(eat.trigger).unwrap_or(Frame::FOREVER),
            ends: starts.checked_add(eat.lasts).unwrap_or(Frame::FOREVER),
            heal: eat.heal.unwrap_or(1.0),
            raised: false,
            consumed: false,
        };
        tick.state_mut().insert(wiring.eating, grabber, eating)?;
        Ok(())
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
    ) -> Result<()> {
        self.advance_eating(tick, ids)?;
        self.scan_pick_ups(tick, ids, living)
    }

    fn advance_eating(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
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
            let alive = |entity: Entity| {
                tick.state().is_live(entity)
                    && !tick.state().contains(ids.doomed, entity)
                    && tick
                        .state()
                        .get(ids.health, entity)
                        .is_some_and(|health| !health.0.is_dead())
            };
            let held = tick
                .state()
                .get(ids.contained, eating.passenger)
                .is_some_and(|contained| contained.container == eater);
            let over = now >= eating.ends;
            if !alive(eater) || over || !(eating.consumed || (held && alive(eating.passenger))) {
                if eating.raised {
                    self.eating_condition(tick, ids, eater, &eating.state, false)?;
                }
                tick.state_mut().remove(wiring.eating, eater);
                continue;
            }
            if !eating.raised && now >= eating.starts {
                eating.raised = true;
                self.eating_condition(tick, ids, eater, &eating.state, true)?;
            }
            if !eating.consumed && now >= eating.consumes {
                eating.consumed = true;
                self.consume(tick, ids, eater, eating.passenger, eating.heal)?;
            }
            tick.state_mut().insert(wiring.eating, eater, eating)?;
        }
        Ok(())
    }

    fn eating_condition(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        eater: Entity,
        state: &Name,
        raise: bool,
    ) -> Result<()> {
        let named = names(vec![state.to_string()]);
        let (add, remove) = if raise {
            (named, names(Vec::new()))
        } else {
            (names(Vec::new()), named)
        };
        self.apply_effects(
            tick,
            ids,
            vec![(eater, Effect::ModelConditions { add, remove })],
        )
    }

    fn consume(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        eater: Entity,
        passenger: Entity,
        heal: f32,
    ) -> Result<()> {
        let mut gained = 0.0;
        if let Some(health) = tick.state_mut().get_mut(ids.health, eater) {
            let before = health.0.current();
            health.0.set_current(before + heal * health.0.max());
            gained = health.0.current() - before;
        }
        if let Some(mut health) = tick.state().get(ids.health, passenger).copied() {
            health.0.set_current(0.0);
            tick.state_mut().insert(ids.health, passenger, health)?;
        }
        diag!(
            "contain: frame {} entity {} eats entity {} and heals {gained:.0}",
            tick.frame().get(),
            eater.index(),
            passenger.index()
        );
        self.resolve_deaths(tick, ids, &[(passenger, 0.0)])
    }

    fn scan_pick_ups(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let now = tick.frame();
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
                if self.pick_up_once(tick, ids, grabber, &module, living)? {
                    break;
                }
            }
        }
        Ok(())
    }

    fn pick_up_once(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        module: &mint_sage_modules::AutoPickUpUpdate,
        living: &[(Foe, PlayerId)],
    ) -> Result<bool> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(false);
        };
        let world = tick.state();
        if world.contains(ids.doomed, grabber) {
            return Ok(false);
        }
        let (Some(owner), Some(entry), Some(health), Some(body)) = (
            world.get(ids.owner, grabber).map(|owner| owner.0),
            world
                .get(ids.kind, grabber)
                .and_then(|kind| self.kinds.get(*kind)),
            world.get(ids.health, grabber).map(|health| health.0),
            world.get(ids.body, grabber).map(|body| body.0.position),
        ) else {
            return Ok(false);
        };
        if health.is_dead() {
            return Ok(false);
        }
        let Some(power) = entry
            .record
            .powers
            .iter()
            .find(|slot| self.power_grabs_passenger(&slot.power))
            .map(|slot| slot.power.clone())
        else {
            return Ok(false);
        };
        let eat = module.eat();
        if eat.is_some_and(|eat| health.current() > eat.my_health * health.max()) {
            return Ok(false);
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
        let target = match best {
            Some((_, target)) => target,
            None if eat.is_none()
                && module
                    .pick_up_filter()
                    .is_some_and(|filter| filter.passes(&[String::from("TREE")], "")) =>
            {
                match self.convert_grabbable_tree(tick, ids, body, reach)? {
                    Some(target) => target,
                    None => return Ok(false),
                }
            }
            None => return Ok(false),
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
            return Ok(false);
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
        Ok(true)
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
