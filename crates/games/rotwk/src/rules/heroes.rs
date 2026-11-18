use mint_sage_economy::{ItemId, ItemTable, Money, QueueRequest};
use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::economy::{Buildable, Purse};
use crate::heroes::{
    discounted, hero_discount, respawn_rule, revive_price, HeldRevive, HeroAvailability, HeroOrder,
    HeroRefusal, HeroRoster, HeroState, ReviveTerms,
};

impl RotwkRules {
    pub(crate) fn unit_command_points_fit(
        &self,
        world: &World,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> bool {
        let (Some(ids), Some(buildable)) = (
            self.ids,
            self.buildables
                .iter()
                .find(|buildable| buildable.kind == kind),
        ) else {
            return true;
        };
        let army_of_dead = self
            .kinds
            .get(kind)
            .is_some_and(|entry| entry.record.has_kind("ARMY_OF_DEAD"));
        let free = crate::economy::points_of(world, Self::economy_ids(ids), player).free();
        crate::heroes::command_points_fit(buildable.command_points, free, army_of_dead)
    }

    pub(super) fn buildable_at(&self, index: u16) -> Option<&Buildable> {
        let index = usize::from(index);
        self.buildables.get(index).or_else(|| {
            index
                .checked_sub(self.buildables.len())
                .and_then(|at| self.hero_buildables.get(at))
        })
    }

    pub(crate) fn hero_buildable_named(&self, label: &str) -> Option<u16> {
        let at = self
            .hero_buildables
            .iter()
            .position(|buildable| buildable.label == label)?;
        u16::try_from(self.buildables.len().checked_add(at)?).ok()
    }

    fn hero_buildable_index(&self, kind: crate::unit::Kind) -> Option<u16> {
        let at = self
            .hero_buildables
            .iter()
            .position(|buildable| buildable.kind == kind)?;
        u16::try_from(self.buildables.len().checked_add(at)?).ok()
    }

    pub(crate) fn roster_of<'w>(
        &self,
        world: &'w World,
        ids: Ids,
        player: PlayerId,
    ) -> Option<&'w HeroRoster> {
        let entity = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
        world.get(ids.hero_roster, entity)
    }

    pub(crate) fn hero_order(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
        button: u8,
    ) -> core::result::Result<HeroOrder, HeroRefusal> {
        if world.get(ids.owner, structure).map(|owner| owner.0) != Some(player) {
            return Err(HeroRefusal::NotOwned);
        }
        let entry = world
            .get(ids.kind, structure)
            .and_then(|kind| self.kinds.get(*kind))
            .ok_or(HeroRefusal::NoButton)?;
        let buttons = entry.revive_buttons_under(self.command_set_override(world, structure));
        let needs = buttons
            .get(usize::from(button))
            .ok_or(HeroRefusal::NoButton)?;
        if !needs.met(
            &self.effective_upgrades_in(world, ids, structure),
            &self.upgrade_bits,
        ) {
            return Err(HeroRefusal::NeedsUpgrade);
        }
        if !world.contains(ids.producer, structure) {
            return Err(HeroRefusal::NoProducer);
        }
        let roster = self
            .roster_of(world, ids, player)
            .ok_or(HeroRefusal::NoHero)?;
        let slot = usize::from(button);
        let hero = roster.0.get(slot).ok_or(HeroRefusal::NoHero)?;
        let kind = hero.kind.ok_or(HeroRefusal::NoHero)?;
        if !self.allowed_to_build(world, ids, player, kind, false) {
            return Err(HeroRefusal::NotAllowed);
        }
        let revive = match hero.state {
            HeroState::Available => None,
            HeroState::Dead { points } => Some(points),
            HeroState::Alive(_) => return Err(HeroRefusal::Fielded),
            HeroState::Queued { .. } => return Err(HeroRefusal::InProduction),
        };
        let record = &self.kinds.get(kind).ok_or(HeroRefusal::NoHero)?.record;
        let held = self.effective_upgrades_in(world, ids, structure);
        let held_name = |name: &str| {
            self.upgrade_bits
                .bit(name)
                .is_some_and(|bit| held.contains(bit))
        };
        let filter_passes = |tokens: &[String]| {
            crate::defeat::ObjectFilter::parse(tokens).passes(&record.kind_of, &record.name)
        };
        let structure_record = &entry.record;
        let (cost_mult, time_mult) = hero_discount(
            &structure_record.production_modifiers,
            revive.is_some(),
            filter_passes,
            held_name,
        );
        let modifiers = crate::economy::cost_modifiers_for(
            world,
            Self::economy_ids(ids),
            &self.kinds,
            &self.upgrade_bits,
            player,
            &crate::economy::PriceTarget::Object {
                kind_of: &record.kind_of,
                name: &record.name,
            },
            self.production_cost_change_percent(player),
        );
        let brutal = self.brutal_cheats_for(player);
        let power_cost = i64::from(self.cah_power_cost(player, kind));
        let held_revive =
            revive.is_some() && self.cah_plan_of(player).is_some() && self.is_cah_kind(kind);
        let seeded = self
            .wotr_run(world)
            .and_then(|run| run.revive_price(player, kind));
        let (base_cost, seconds) = match revive {
            None => (
                record
                    .build_cost
                    .and_then(crate::templates::whole)
                    .filter(|cost| *cost >= 0)
                    .map(|cost| cost.saturating_add(power_cost)),
                record
                    .build_time
                    .filter(|s| (0.0..=crate::templates::MAX_BUILD_SECONDS).contains(s)),
            ),
            Some(_) if !held_revive && seeded.is_some() => {
                let (cost, seconds) = seeded.unwrap_or_default();
                (Some(cost), Some(seconds))
            }
            Some(points) => {
                let rank = self.rank_at(kind, points);
                let rule = respawn_rule(&record.respawn, &record.respawn_entries, rank);
                let cost = if held_revive {
                    record
                        .build_cost
                        .and_then(crate::templates::whole)
                        .filter(|cost| *cost >= 0)
                } else {
                    rule.and_then(|rule| rule.cost)
                        .and_then(crate::templates::whole)
                        .filter(|cost| *cost >= 0)
                };
                (
                    cost,
                    rule.and_then(|rule| rule.time_ms)
                        .map(|ms| ms / 1000.0)
                        .filter(|s| (0.0..=crate::templates::MAX_BUILD_SECONDS).contains(s)),
                )
            }
        };
        let (Some(base_cost), Some(seconds)) = (base_cost, seconds) else {
            return Err(HeroRefusal::NoPrice);
        };
        let (cost, build_time) = if revive.is_some() {
            revive_price(
                base_cost,
                seconds,
                &ReviveTerms {
                    player: modifiers,
                    slot: (cost_mult, time_mult),
                    brutal,
                    held: held_revive.then_some(HeldRevive {
                        sum: power_cost,
                        discount_percent: self.cah_revival_discount,
                    }),
                },
            )
        } else {
            let player_factor =
                (1.0 + modifiers.production_cost_change_percent) * modifiers.kind_of_modifier;
            discounted(
                base_cost,
                seconds,
                (cost_mult * player_factor, time_mult),
                brutal,
            )
        };
        let purse = crate::economy::purse_of(world, Self::economy_ids(ids), player);
        if !purse.can_afford(cost) {
            return Err(HeroRefusal::CantAfford { cost });
        }
        if let Some(producer) = world.get(ids.producer, structure) {
            let depth = u32::try_from(producer.queue.entries().len()).unwrap_or(u32::MAX);
            if depth >= producer.queue.max_entries() {
                return Err(HeroRefusal::QueueFull);
            }
            let needed = self
                .hero_buildable_index(kind)
                .and_then(|item| self.buildable_at(item))
                .map_or(0, |buildable| buildable.command_points);
            let free = crate::economy::points_of(world, Self::economy_ids(ids), player).free();
            if !crate::heroes::command_points_fit(needed, free, record.has_kind("ARMY_OF_DEAD")) {
                return Err(HeroRefusal::CommandPoints { needed, free });
            }
        }
        Ok(HeroOrder {
            slot,
            kind,
            revive,
            cost,
            build_time,
        })
    }

    pub(crate) fn rank_at(&self, kind: crate::unit::Kind, points: u32) -> u16 {
        self.level_tables
            .get(kind)
            .and_then(|table| table.level(table.index_for(points)))
            .map_or(1, |level| level.rank)
    }

    pub(super) fn recruit_hero(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
    ) -> Result<()> {
        let selected = self.selection(tick, ids, player);
        let Some(structure) = selected.into_iter().find(|entity| {
            let world = tick.state();
            world.get(ids.owner, *entity).map(|owner| owner.0) == Some(player)
                && world
                    .get(ids.kind, *entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| {
                        !entry
                            .revive_buttons_under(self.command_set_override(world, *entity))
                            .is_empty()
                    })
        }) else {
            return Ok(());
        };
        let order = match self.hero_order(tick.state(), ids, player, structure, button) {
            Ok(order) => order,
            Err(refusal) => {
                diag!(
                    "hero: button {} at entity {} refused - {}",
                    u16::from(button).saturating_add(1),
                    structure.index(),
                    refusal.message()
                );
                return Ok(());
            }
        };
        let Some(item) = self.hero_buildable_index(order.kind) else {
            return Ok(());
        };
        let command_points = self.buildable_at(item).map_or(0, |b| b.command_points);
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
        else {
            return Ok(());
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(Money::new(0), |held| held.0);
        let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) else {
            return Ok(());
        };
        let request = QueueRequest {
            item: ItemId::new(u32::from(item)),
            cost: order.cost,
            build_time: order.build_time,
            quantity: 1,
            command_points,
            kind: mint_sage_economy::EntryKind::Revival {
                builder: self
                    .buildable_at(item)
                    .is_some_and(|buildable| buildable.builder),
            },
        };
        if let Err(refusal) = producer.queue.queue(request, &mut purse) {
            diag!(
                "hero: {} at entity {} refused - {}: {refusal}",
                self.kind_name(Some(order.kind)),
                structure.index(),
                HeroRefusal::QueueRefused.message()
            );
            return Ok(());
        }
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.spend(crate::match_stats::SpendKind::Heroes, order.cost);
        });
        self.set_hero_state(
            tick,
            ids,
            player_entity,
            order.slot,
            HeroState::Queued {
                structure,
                revive: order.revive,
            },
        )?;
        diag!(
            "hero: {} player {} {} {} at entity {} for {} - {} frame(s)",
            tick.frame(),
            player.get(),
            if order.revive.is_some() {
                "revives"
            } else {
                "recruits"
            },
            self.kind_name(Some(order.kind)),
            structure.index(),
            order.cost,
            order.build_time.get()
        );
        Ok(())
    }

    pub(super) fn set_hero_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player_entity: Entity,
        slot: usize,
        state: HeroState,
    ) -> Result<()> {
        if let Some(roster) = tick.state_mut().get_mut(ids.hero_roster, player_entity) {
            if let Some(entry) = roster.0.get_mut(slot) {
                entry.state = state;
            }
        }
        Ok(())
    }

    pub(super) fn revive_exit_structure(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        queued_at: Entity,
        kind: crate::unit::Kind,
    ) -> Entity {
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return queued_at;
        };
        if record.auto_respawn_at_object_filter.is_empty() {
            return queued_at;
        }
        let is_revive = self.roster_of(world, ids, owner).is_some_and(|roster| {
            roster.0.iter().any(|entry| {
                entry.kind == Some(kind)
                    && matches!(
                        entry.state,
                        HeroState::Queued { structure, revive: Some(_) } if structure == queued_at
                    )
            })
        });
        if !is_revive {
            return queued_at;
        }
        let filter = crate::defeat::ObjectFilter::parse(&record.auto_respawn_at_object_filter);
        let Some(store) = world.store(ids.owner) else {
            return queued_at;
        };
        let mut candidates: Vec<Entity> = store
            .iter()
            .filter(|(entity, held)| {
                held.0 == owner && world.is_live(*entity) && !world.contains(ids.doomed, *entity)
            })
            .map(|(entity, _)| entity)
            .collect();
        candidates.sort_unstable_by_key(|entity| entity.index());
        candidates
            .into_iter()
            .find(|entity| {
                world
                    .get(ids.kind, *entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| filter.passes(&entry.record.kind_of, &entry.record.name))
            })
            .unwrap_or(queued_at)
    }

    pub(super) fn hero_delivered(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        owner: PlayerId,
        hero: Entity,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, hero).copied() else {
            return Ok(());
        };
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let Some((slot, revive)) = self.roster_of(tick.state(), ids, owner).and_then(|roster| {
            roster
                .0
                .iter()
                .enumerate()
                .find_map(|(at, entry)| match entry.state {
                    HeroState::Queued {
                        structure: queued_at,
                        revive,
                    } if queued_at == structure && entry.kind == Some(kind) => Some((at, revive)),
                    _ => None,
                })
        }) else {
            return Ok(());
        };
        self.set_hero_state(tick, ids, player_entity, slot, HeroState::Alive(hero))?;
        self.bind_create_a_hero(tick, ids, owner, hero)?;
        self.play_hero_exit_fx(tick, ids, kind, structure, hero, revive.is_some());
        let Some(points) = revive else {
            diag!(
                "hero: {} {} came out of entity {} as entity {}",
                tick.frame(),
                self.kind_name(Some(kind)),
                structure.index(),
                hero.index()
            );
            return Ok(());
        };
        self.restore_experience(tick, ids, hero, points)?;
        let health_fraction = self
            .kinds
            .get(kind)
            .and_then(|entry| {
                let rank = self.rank_of(tick.state(), hero).unwrap_or(1);
                respawn_rule(&entry.record.respawn, &entry.record.respawn_entries, rank)?.health
            })
            .unwrap_or(1.0);
        if let Some(mut health) = tick.state().get(ids.health, hero).copied() {
            health
                .0
                .set_current(health.0.max() * health_fraction.clamp(0.0, 1.0));
            tick.state_mut().insert(ids.health, hero, health)?;
        }
        diag!(
            "hero: {} {} revived as entity {} - rank {} with {points} point(s)",
            tick.frame(),
            self.kind_name(Some(kind)),
            hero.index(),
            self.rank_of(tick.state(), hero).unwrap_or(0)
        );
        Ok(())
    }

    fn play_hero_exit_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: crate::unit::Kind,
        structure: Entity,
        hero: Entity,
        is_revive: bool,
    ) {
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return;
        };
        let list = if is_revive {
            record.respawn_fx.as_deref()
        } else {
            record.initial_spawn_fx.as_deref()
        };
        let Some(list) = list else {
            return;
        };
        let Some(at) = tick
            .state()
            .get(ids.body, structure)
            .map(|body| body.0.position)
        else {
            return;
        };
        diag!(
            "hero: {} {} plays {list} ({}) for entity {}",
            tick.frame(),
            self.kind_name(Some(kind)),
            if is_revive {
                "RespawnFX"
            } else {
                "InitialSpawnFX"
            },
            hero.index()
        );
        self.emit_fx_object_named(tick, ids, list, at, crate::fx::FxSource::Power, hero, None);
    }

    fn play_hero_death_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: crate::unit::Kind,
        hero: Entity,
        at: mint_movement::Vec2,
    ) {
        let Some(list) = self
            .kinds
            .get(kind)
            .and_then(|entry| entry.record.death_fx.as_deref())
        else {
            return;
        };
        diag!(
            "hero: {} {} plays {list} (DeathFX) for entity {}",
            tick.frame(),
            self.kind_name(Some(kind)),
            hero.index()
        );
        self.emit_fx_object_named(tick, ids, list, at, crate::fx::FxSource::Power, hero, None);
    }

    pub(super) fn note_hero_losses(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let rosters: Vec<(Entity, HeroRoster)> = tick
            .state()
            .store(ids.hero_roster)
            .map(|store| store.iter().map(|(e, r)| (e, r.clone())).collect())
            .unwrap_or_default();
        let mut died_holding_cah: Vec<Entity> = Vec::new();
        for (player_entity, roster) in rosters {
            let mut next = roster.clone();
            for entry in &mut next.0 {
                let world = tick.state();
                let gone =
                    |entity: Entity| !world.is_live(entity) || world.contains(ids.doomed, entity);
                entry.state = match entry.state {
                    HeroState::Alive(hero) if gone(hero) => {
                        let points = self.experience_of(world, ids, hero).map_or(0, |(_, p)| p);
                        let at = world.get(ids.body, hero).map(|body| body.0.position);
                        diag!(
                            "hero: {} {} (entity {}) died holding {points} point(s)",
                            tick.frame(),
                            self.kind_name(entry.kind),
                            hero.index()
                        );
                        if let Some(kind) = entry.kind {
                            self.note_wotr_fallen(tick, ids, player_entity, hero, kind);
                        }
                        if let (Some(kind), Some(at)) = (entry.kind, at) {
                            self.play_hero_death_fx(tick, ids, kind, hero, at);
                        }
                        if entry.kind.is_some_and(|kind| self.is_cah_kind(kind)) {
                            died_holding_cah.push(player_entity);
                        }
                        HeroState::Dead { points }
                    }
                    HeroState::Queued { structure, revive } if gone(structure) => match revive {
                        Some(points) => HeroState::Dead { points },
                        None => HeroState::Available,
                    },
                    state => state,
                };
            }
            if next != roster {
                tick.state_mut()
                    .insert(ids.hero_roster, player_entity, next)?;
            }
        }
        for player_entity in died_holding_cah {
            self.cah_hero_died(tick, ids, player_entity)?;
        }
        Ok(())
    }

    pub(super) fn hero_cancelled(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        owner: PlayerId,
        item: u16,
    ) -> Result<()> {
        let Some(kind) = self.buildable_at(item).map(|b| b.kind) else {
            return Ok(());
        };
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let Some((slot, revive)) = self.roster_of(tick.state(), ids, owner).and_then(|roster| {
            roster
                .0
                .iter()
                .enumerate()
                .find_map(|(at, entry)| match entry.state {
                    HeroState::Queued {
                        structure: queued_at,
                        revive,
                    } if queued_at == structure && entry.kind == Some(kind) => Some((at, revive)),
                    _ => None,
                })
        }) else {
            return Ok(());
        };
        let state = match revive {
            Some(points) => HeroState::Dead { points },
            None => HeroState::Available,
        };
        self.set_hero_state(tick, ids, player_entity, slot, state)
    }

    pub(crate) fn hero_progress(
        &self,
        world: &World,
        structure: Entity,
        button: u8,
    ) -> Option<f32> {
        let ids = self.ids?;
        let owner = world.get(ids.owner, structure)?.0;
        let slot = self
            .roster_of(world, ids, owner)?
            .0
            .get(usize::from(button))?;
        let kind = match slot.state {
            HeroState::Queued {
                structure: queued_at,
                ..
            } if queued_at == structure => slot.kind?,
            _ => return None,
        };
        let item = self.hero_buildable_index(kind)?;
        world
            .get(ids.producer, structure)?
            .queue
            .first_of(ItemTable::Objects, ItemId::new(u32::from(item)))
            .map(|entry| entry.fraction())
    }

    pub(super) fn cancel_production(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        structures: &[Entity],
        pick: &dyn Fn(&crate::economy::Producer) -> Option<mint_sage_economy::ProductionId>,
    ) -> Result<()> {
        let label = |item: u32| {
            u16::try_from(item)
                .ok()
                .and_then(|item| self.buildable_at(item))
                .map_or_else(|| "a unit".to_owned(), |b| b.label.clone())
        };
        let (lines, cancelled) = crate::economy::cancel_at(
            tick,
            Self::economy_ids(ids),
            player,
            structures,
            pick,
            &label,
        )?;
        for line in lines {
            diag!("{line}");
        }
        for (structure, item) in cancelled {
            if let Ok(item) = u16::try_from(item) {
                self.hero_cancelled(tick, ids, structure, player, item)?;
            }
        }
        Ok(())
    }

    pub(super) fn cancel_hero(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
    ) -> Result<()> {
        let queued = self
            .roster_of(tick.state(), ids, player)
            .and_then(|roster| roster.0.get(usize::from(button)))
            .and_then(|slot| match slot.state {
                HeroState::Queued { structure, .. } => Some((structure, slot.kind?)),
                _ => None,
            });
        let Some((structure, kind)) = queued else {
            return Ok(());
        };
        let Some(item) = self.hero_buildable_index(kind) else {
            return Ok(());
        };
        let entry = |producer: &crate::economy::Producer| {
            producer
                .queue
                .last_of(ItemTable::Objects, ItemId::new(u32::from(item)))
                .map(|entry| entry.id)
        };
        self.cancel_production(tick, ids, player, &[structure], &entry)
    }

    pub(super) fn cancel_heroes_missing_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
    ) -> Result<()> {
        let Some(roster) = self.roster_of(tick.state(), ids, owner) else {
            return Ok(());
        };
        let queued: Vec<(u8, Entity)> = roster
            .0
            .iter()
            .enumerate()
            .filter_map(|(slot, hero)| match hero.state {
                HeroState::Queued { structure, .. } => {
                    u8::try_from(slot).ok().map(|slot| (slot, structure))
                }
                _ => None,
            })
            .collect();
        for (button, structure) in queued {
            let world = tick.state();
            let Some(entry) = world
                .get(ids.kind, structure)
                .and_then(|kind| self.kinds.get(*kind))
            else {
                continue;
            };
            let buttons = entry.revive_buttons_under(self.command_set_override(world, structure));
            let Some(needs) = buttons.get(usize::from(button)) else {
                continue;
            };
            if !needs.met(
                &self.effective_upgrades_in(world, ids, structure),
                &self.upgrade_bits,
            ) {
                self.cancel_hero(tick, ids, owner, button)?;
            }
        }
        Ok(())
    }

    pub(crate) fn hero_page(
        &self,
        world: &World,
        player: PlayerId,
        structure: Option<Entity>,
    ) -> Vec<HeroAvailability> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(roster) = self.roster_of(world, ids, player) else {
            return Vec::new();
        };
        roster
            .0
            .iter()
            .enumerate()
            .map(|(at, entry)| {
                let fielded = hero_page_fielded(entry.state);
                let points = match entry.state {
                    HeroState::Dead { points }
                    | HeroState::Queued {
                        revive: Some(points),
                        ..
                    } => points,
                    _ => 0,
                };
                let answer = structure.and_then(|structure| {
                    u8::try_from(at)
                        .ok()
                        .map(|button| self.hero_order(world, ids, player, structure, button))
                });
                let (affordable, allowed, cost) = match answer {
                    Some(Ok(order)) => (true, true, Some(order.cost)),
                    Some(Err(HeroRefusal::CantAfford { cost })) => (false, true, Some(cost)),
                    Some(Err(HeroRefusal::CommandPoints { .. })) => (false, true, None),
                    Some(Err(HeroRefusal::NotAllowed)) => (true, false, None),
                    _ => (true, true, None),
                };
                HeroAvailability {
                    fielded,
                    affordable,
                    allowed,
                    cost,
                    revive: matches!(
                        entry.state,
                        HeroState::Dead { .. }
                            | HeroState::Queued {
                                revive: Some(_),
                                ..
                            }
                    ),
                    level: entry
                        .kind
                        .and_then(|kind| self.level_tables.get(kind))
                        .and_then(|table| table.level(table.index_for(points)))
                        .map_or(1, |level| level.rank),
                }
            })
            .collect()
    }
}

#[must_use]
fn hero_page_fielded(state: HeroState) -> bool {
    matches!(state, HeroState::Alive(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::World;

    fn any_entity() -> Entity {
        let mut world = World::new();
        world.spawn().expect("a fresh world always has room")
    }

    #[test]
    fn only_an_alive_hero_hides_the_page_slot() {
        let entity = any_entity();
        assert!(!hero_page_fielded(HeroState::Available));
        assert!(!hero_page_fielded(HeroState::Dead { points: 0 }));
        assert!(!hero_page_fielded(HeroState::Queued {
            structure: entity,
            revive: None,
        }));
        assert!(hero_page_fielded(HeroState::Alive(entity)));
    }
}
