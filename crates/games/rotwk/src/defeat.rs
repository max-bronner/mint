use mint_sim::{Component, ComponentId, Entity, Frame, PlayerId, Result, StateHasher, Tick, World};

use crate::combat::{Doomed, Health};
use crate::rules::death::{mark_dying, DyingIds};
use crate::templates::KindTable;
use crate::unit::{Kind, Owner};

pub(crate) use mint_sage_modules::ObjectFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DefeatMode {
    #[default]
    UnitFilter,
    Dozer,
}

impl DefeatMode {
    pub(crate) const fn of_match(wotr: bool) -> Self {
        if wotr {
            Self::Dozer
        } else {
            Self::UnitFilter
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VictoryFilters<'a> {
    pub(crate) structure: &'a ObjectFilter,
    pub(crate) unit: &'a ObjectFilter,
    pub(crate) mode: DefeatMode,
}

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
    latched: Vec<bool>,
}

impl DefeatFacts {
    fn is_latched(&self, player: PlayerId) -> bool {
        self.latched
            .get(usize::from(player.get()))
            .copied()
            .unwrap_or(false)
    }

    fn latch(&mut self, player: PlayerId) {
        let slot = usize::from(player.get());
        if self.latched.len() <= slot {
            self.latched.resize(slot.saturating_add(1), false);
        }
        if let Some(flag) = self.latched.get_mut(slot) {
            *flag = true;
        }
    }

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
        if self.latched.iter().any(|flag| *flag) {
            hasher.write_usize(self.latched.len());
            for flag in &self.latched {
                hasher.write_bool(*flag);
            }
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
    filters: VictoryFilters<'_>,
    players: u8,
    alliances: &crate::alliance::Alliances,
) -> Result<()> {
    if players < 2 || tick.state().get(ids.outcome, clock).is_some() {
        return Ok(());
    }
    let mut remaining: Vec<PlayerId> = Vec::new();
    for slot in 0..players {
        let player = PlayerId::new(slot);
        if is_alive(tick.state(), ids, kinds, filters, player) {
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

pub(crate) fn latch_defeat(tick: &mut Tick<'_>, ids: DefeatIds, player: PlayerId) -> Result<()> {
    let Some((clock, mut facts)) = tick
        .state()
        .store(ids.facts)
        .and_then(|store| store.iter().next())
        .map(|(entity, facts)| (entity, facts.clone()))
    else {
        return Ok(());
    };
    facts.latch(player);
    tick.state_mut().insert(ids.facts, clock, facts)?;
    Ok(())
}

pub(crate) fn has_been_defeated(
    world: &World,
    ids: DefeatIds,
    kinds: &KindTable,
    filters: VictoryFilters<'_>,
    player: PlayerId,
) -> bool {
    !is_alive(world, ids, kinds, filters, player)
}

fn is_alive(
    world: &World,
    ids: DefeatIds,
    kinds: &KindTable,
    filters: VictoryFilters<'_>,
    player: PlayerId,
) -> bool {
    if world
        .store(ids.facts)
        .and_then(|store| store.iter().next())
        .is_some_and(|(_, facts)| facts.is_latched(player))
    {
        return false;
    }
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
        let record = &entry.record;
        if record.has_kind("PROJECTILE") || record.has_kind("IGNORE_FOR_VICTORY") {
            continue;
        }
        let passes = |filter: &ObjectFilter| filter.passes(&record.kind_of, &record.name);
        if !record.has_kind("MINE") && passes(filters.structure) {
            return true;
        }
        if !record.has_kind("STRUCTURE") {
            let unit_alive = match filters.mode {
                DefeatMode::UnitFilter => filters.unit.matches_something() && passes(filters.unit),
                DefeatMode::Dozer => record.has_kind("DOZER"),
            };
            if unit_alive {
                return true;
            }
        }
    }
    false
}

pub(crate) fn kill_every_object_of(
    tick: &mut Tick<'_>,
    ids: DefeatIds,
    player: PlayerId,
) -> Result<()> {
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
