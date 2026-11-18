use mint_sim::{Component, ComponentId, Entity, Frame, PlayerId, Result, StateHasher, Tick, World};

use crate::combat::{Doomed, Health};
use crate::rules::death::{mark_dying, DyingIds};
use crate::templates::KindTable;
use crate::unit::{Kind, Owner};

pub(crate) use mint_sage_modules::ObjectFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatchOutcome {
    pub(crate) winner: PlayerId,
    pub(crate) frame: Frame,
}

impl Component for MatchOutcome {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.winner.hash_into(hasher);
        self.frame.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DefeatIds {
    pub(crate) owner: ComponentId<Owner>,
    pub(crate) kind: ComponentId<Kind>,
    pub(crate) health: ComponentId<Health>,
    pub(crate) outcome: ComponentId<MatchOutcome>,
    pub(crate) facts: ComponentId<DefeatFacts>,
    pub(crate) dying: DyingIds,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DefeatFacts {
    frame: Frame,
    newly_defeated: Vec<PlayerId>,
    reported: Vec<bool>,
}

impl DefeatFacts {
    fn mark(&mut self, now: Frame, player: PlayerId) {
        if self.frame != now {
            self.newly_defeated.clear();
            self.frame = now;
        }
        let slot = usize::from(player.get());
        if self.reported.len() <= slot {
            self.reported.resize(slot.saturating_add(1), false);
        }
        if let Some(seen) = self.reported.get_mut(slot) {
            if !*seen {
                *seen = true;
                self.newly_defeated.push(player);
            }
        }
    }

    pub(crate) fn this_frame(&self, now: Frame) -> &[PlayerId] {
        if self.frame == now {
            &self.newly_defeated
        } else {
            &[]
        }
    }
}

impl Component for DefeatFacts {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.newly_defeated.len());
        for player in &self.newly_defeated {
            player.hash_into(hasher);
        }
        hasher.write_usize(self.reported.len());
        for seen in &self.reported {
            hasher.write_bool(*seen);
        }
    }
}

pub(crate) fn defeat_facts(world: &World, ids: DefeatIds, now: Frame) -> &[PlayerId] {
    world
        .store(ids.facts)
        .and_then(|store| store.iter().next())
        .map_or(&[] as &[PlayerId], |(_, facts)| facts.this_frame(now))
}

pub(crate) fn advance_victory(
    tick: &mut Tick<'_>,
    ids: DefeatIds,
    clock: Entity,
    kinds: &KindTable,
    structure_filter: &ObjectFilter,
    players: u8,
    alliances: &crate::alliance::Alliances,
) -> Result<()> {
    if players < 2 || tick.state().get(ids.outcome, clock).is_some() {
        return Ok(());
    }
    let mut remaining: Vec<PlayerId> = Vec::new();
    for slot in 0..players {
        let player = PlayerId::new(slot);
        if has_passing_structure(tick.state(), ids, kinds, structure_filter, player) {
            remaining.push(player);
        } else {
            kill_every_object_of(tick, ids, player)?;
            mark_newly_defeated(tick, ids, clock, player)?;
        }
    }
    if let (Some(&winner), true) = (remaining.first(), alliances.one_side(&remaining)) {
        let frame = tick.frame();
        tick.state_mut()
            .insert(ids.outcome, clock, MatchOutcome { winner, frame })?;
    }
    Ok(())
}

fn mark_newly_defeated(
    tick: &mut Tick<'_>,
    ids: DefeatIds,
    clock: Entity,
    player: PlayerId,
) -> Result<()> {
    let now = tick.frame();
    if let Some(mut facts) = tick.state().get(ids.facts, clock).cloned() {
        facts.mark(now, player);
        tick.state_mut().insert(ids.facts, clock, facts)?;
    }
    Ok(())
}

pub(crate) fn has_been_defeated(
    world: &World,
    ids: DefeatIds,
    kinds: &KindTable,
    filter: &ObjectFilter,
    player: PlayerId,
) -> bool {
    !has_passing_structure(world, ids, kinds, filter, player)
}

fn has_passing_structure(
    world: &World,
    ids: DefeatIds,
    kinds: &KindTable,
    filter: &ObjectFilter,
    player: PlayerId,
) -> bool {
    let Some(store) = world.store(ids.owner) else {
        return false;
    };
    for (entity, owner) in store.iter() {
        if owner.0 != player || !world.is_live(entity) {
            continue;
        }
        if world.contains(ids.dying.doomed, entity) {
            continue;
        }
        if world
            .get(ids.health, entity)
            .is_some_and(|health| health.0.is_dead())
        {
            continue;
        }
        let Some(kind) = world.get(ids.kind, entity) else {
            continue;
        };
        let Some(entry) = kinds.get(*kind) else {
            continue;
        };
        if filter.passes(&entry.record.kind_of, &entry.record.name) {
            return true;
        }
    }
    false
}

fn kill_every_object_of(tick: &mut Tick<'_>, ids: DefeatIds, player: PlayerId) -> Result<()> {
    let now = tick.frame();
    let entities: Vec<Entity> = tick
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .filter(|(_, owner)| owner.0 == player)
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default();
    for entity in entities {
        if !tick.state().is_live(entity) || tick.state().contains(ids.dying.doomed, entity) {
            continue;
        }
        mark_dying(
            tick.state_mut(),
            ids.dying,
            entity,
            false,
            Doomed {
                destroy_at: now,
                death_index: 0,
                death_conditions: mint_sage_scene::Conditions::NONE,
            },
        )?;
    }
    Ok(())
}
