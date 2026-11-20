use mint_sage_combat::Point3;
use mint_sage_modules::effects::{Effect, Name};
use mint_sage_modules::object_scripts::{
    BroadcastKind, Broadcaster, EventArg, EventSource, ObjectFacts,
};
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{Ids, RotwkRules};
use crate::diag::diag;

const GENERIC_EVENT: &str = "OnGenericEvent";

const MAX_BROADCAST_DEPTH: u32 = 10;

struct Dispatch<'a, 'b> {
    rules: &'a RotwkRules,
    tick: &'a mut Tick<'b>,
    ids: Ids,
    broadcaster: Entity,
    depth: u32,
    error: Option<mint_sim::Error>,
}

impl Broadcaster for Dispatch<'_, '_> {
    fn broadcast(&mut self, kind: BroadcastKind, event: &str, radius: f32) {
        if self.error.is_some() || self.depth >= MAX_BROADCAST_DEPTH {
            return;
        }
        for receiver in self.rules.broadcast_receivers(
            self.tick.state(),
            self.ids,
            self.broadcaster,
            kind,
            radius,
        ) {
            if let Err(error) = self.rules.fire_object_event_at(
                self.tick,
                self.ids,
                receiver,
                event,
                None,
                Some(self.broadcaster),
                None,
                self.depth.saturating_add(1),
            ) {
                self.error = Some(error);
                return;
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn fire_state_enter_events(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        added: &[Name],
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(entry) = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return Ok(());
        };
        if entry.record.lua_event_states.is_empty() {
            return Ok(());
        }
        let held = tick
            .state()
            .get(wiring.state, entity)
            .map(|state| state.model_condition_names())
            .unwrap_or_default();
        let fired: Vec<Option<String>> = entry
            .record
            .lua_event_states
            .iter()
            .filter(|state| {
                !state.conditions.is_empty()
                    && state.conditions.iter().any(|condition| {
                        added
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(condition))
                    })
                    && state.conditions.iter().all(|condition| {
                        held.iter().any(|name| name.eq_ignore_ascii_case(condition))
                    })
            })
            .map(|state| state.data.clone())
            .collect();
        for data in fired {
            self.fire_object_event(tick, ids, entity, GENERIC_EVENT, data.as_deref())?;
        }
        Ok(())
    }

    pub(super) fn fire_object_event(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        event: &str,
        data: Option<&str>,
    ) -> Result<()> {
        self.fire_object_event_at(tick, ids, entity, event, data, None, None, 0)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_object_event_at(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        event: &str,
        data: Option<&str>,
        other: Option<Entity>,
        third: Option<EventArg>,
        depth: u32,
    ) -> Result<()> {
        let Some(scripts) = self.object_scripts.as_ref() else {
            return Ok(());
        };
        let Some(entry) = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return Ok(());
        };
        let Some(list) = tick
            .state()
            .get(ids.events_list, entity)
            .map(|placed| placed.0.clone())
            .or_else(|| entry.record.lua_events.clone())
        else {
            return Ok(());
        };
        let owner = |of: Entity| tick.state().get(ids.owner, of).map(|owner| owner.0);
        let capturing = tick
            .state()
            .get(ids.capturer, entity)
            .map(|capturer| capturer.0)
            .and_then(owner);
        let conditions = self
            .modules
            .as_ref()
            .and_then(|wiring| tick.state().get(wiring.state, entity))
            .map(|state| state.model_condition_names())
            .unwrap_or_default();
        let facts = ObjectFacts {
            side: owner(entity).and_then(|player| self.side_of(player)),
            capturing_side: capturing.and_then(|player| self.side_of(player)),
            conditions: &conditions,
        };
        let source = EventSource {
            this: Some(entity),
            other,
            third,
        };
        let mut dispatch = Dispatch {
            rules: self,
            tick: &mut *tick,
            ids,
            broadcaster: entity,
            depth,
            error: None,
        };
        let run = scripts.run(&list, event, data, &facts, source, Some(&mut dispatch));
        let nested_error = dispatch.error.take();
        let Some(run) = run else {
            return nested_error.map_or(Ok(()), Err);
        };
        let frame = tick.frame().get();
        for call in &run.refused {
            diag!(
                "lua: frame {frame} entity {} handler {} calls {call}, which is not bound (B41)",
                entity.index(),
                run.function
            );
        }
        if let Some(error) = &run.error {
            diag!("lua: frame {frame} entity {} {error}", entity.index());
        }
        diag!(
            "lua: frame {frame} entity {} {event} -> {} ({} effect(s))",
            entity.index(),
            run.function,
            run.effects.len()
        );
        let mut effects = Vec::new();
        for effect in run.effects {
            if let Effect::PulseOwnEmotion(kind) = effect {
                self.pulse_emotion(tick, ids, entity, kind, other, super::emotions::ONE_FRAME)?;
            } else {
                effects.push((entity, effect));
            }
        }
        self.apply_effects(tick, ids, effects)?;
        nested_error.map_or(Ok(()), Err)
    }

    pub(super) fn broadcast_receivers(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        from: Entity,
        kind: BroadcastKind,
        radius: f32,
    ) -> Vec<Entity> {
        let Some(body) = world.get(ids.body, from) else {
            return Vec::new();
        };
        let sender = world.get(ids.owner, from).map(|owner| owner.0);
        let origin = Point3::new(
            body.0.position.x,
            body.0.position.y,
            Self::feet(world, ids, from),
        );
        let Some(bodies) = world.store(ids.body) else {
            return Vec::new();
        };
        let mut hit: Vec<(f32, Entity)> = bodies
            .iter()
            .filter_map(|(candidate, candidate_body)| {
                if !world.is_live(candidate) {
                    return None;
                }
                let there = Point3::new(
                    candidate_body.0.position.x,
                    candidate_body.0.position.y,
                    Self::feet(world, ids, candidate),
                );
                let distance_squared = origin.distance_squared_3d(there);
                if distance_squared > radius * radius {
                    return None;
                }
                let relation = self.object_victim_relation(world, from, candidate);
                let admitted = match kind {
                    BroadcastKind::Enemies => {
                        relation == mint_sage_combat::victim::Relation::Enemies
                            && sender.is_none_or(|player| {
                                self.not_hidden_from(world, ids, player, candidate)
                            })
                    }
                    BroadcastKind::Allies => relation == mint_sage_combat::victim::Relation::Allies,
                    BroadcastKind::Civilians => {
                        relation == mint_sage_combat::victim::Relation::Neutral
                    }
                    BroadcastKind::Units => true,
                };
                admitted.then_some((distance_squared, candidate))
            })
            .collect();
        hit.sort_unstable_by(|(left, left_entity), (right, right_entity)| {
            left.total_cmp(right)
                .then_with(|| left_entity.cmp(right_entity))
        });
        hit.into_iter().map(|(_, entity)| entity).collect()
    }

    pub(crate) fn frame_event_parts(
        &self,
        kind: crate::unit::Kind,
        data: &str,
    ) -> Option<Vec<(String, bool, bool)>> {
        let scripts = self.object_scripts.as_ref()?;
        let list = self.kinds.get(kind)?.record.lua_events.as_deref()?;
        let run = scripts.run(
            list,
            GENERIC_EVENT,
            Some(data),
            &ObjectFacts::default(),
            EventSource::default(),
            None,
        )?;
        if run.error.is_some() || !run.refused.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        for effect in &run.effects {
            let Effect::SubObjects {
                show,
                hide,
                permanent,
            } = effect
            else {
                return None;
            };
            parts.extend(show.iter().map(|name| (name.to_string(), true, *permanent)));
            parts.extend(
                hide.iter()
                    .map(|name| (name.to_string(), false, *permanent)),
            );
        }
        Some(parts)
    }

    pub(super) fn play_object_sound(&self, tick: &Tick<'_>, entity: Entity, name: &str) {
        let frame = tick.frame().get();
        diag!(
            "lua: frame {frame} entity {} plays sound {name}",
            entity.index()
        );
    }

    fn side_of(&self, player: PlayerId) -> Option<&str> {
        self.player_sides
            .iter()
            .find(|(held, _)| *held == player)
            .map(|(_, side)| side.as_str())
    }
}
