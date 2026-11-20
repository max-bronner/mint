use std::sync::Arc;

use mint_sage_modules::effects::Names;
use mint_sage_modules::Effect;
use mint_sim::{Entity, Frame, PlayerId, Result, Tick, World};

use super::spawning::Placement;
use super::terrain_fire::{singleton_of, FireLighting};
use super::{Ids, RotwkRules};
use crate::castle::{pad_is_occupied, CastleMember, PadOccupant, PendingUnpack};
use crate::construction::{footprint_of, Construction};
use crate::diag::diag;
use crate::economy::Purse;
use crate::unit::Kind;

impl RotwkRules {
    pub(super) fn castle_build_complete(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        site: Entity,
        created: Frame,
    ) -> Result<()> {
        let Some(castle) = tick
            .state()
            .get(ids.kind, site)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.castle.as_ref())
        else {
            return Ok(());
        };
        let (instant, max_radius) = (castle.instant_unpack, castle.max_radius);
        let names = |name: &str| -> Names { vec![Arc::<str>::from(name)].into() };
        self.apply_effects(
            tick,
            ids,
            vec![
                (
                    site,
                    Effect::StatusBits {
                        set: names("UNATTACKABLE"),
                        clear: Vec::new().into(),
                    },
                ),
                (
                    site,
                    Effect::ModelConditions {
                        add: names("INVULNERABLE"),
                        remove: Vec::new().into(),
                    },
                ),
            ],
        )?;
        if let (Some(radius), Some(position)) = (
            max_radius,
            tick.state().get(ids.body, site).map(|body| body.0.position),
        ) {
            if let Some(grid) = singleton_of(tick.state(), ids.fire_grid) {
                if let Some(grid) = tick.state_mut().get_mut(ids.fire_grid, grid) {
                    grid.light(
                        &FireLighting::clear_fuel(radius),
                        (position.x, position.y),
                        None,
                        &|x, y| self.passability.underwater(mint_movement::Vec2::new(x, y)),
                    );
                }
            }
        }
        let completed = tick.frame();
        if instant {
            tick.state_mut().insert(
                ids.pending_unpack,
                site,
                PendingUnpack::after_creation(created, completed),
            )?;
        }
        Ok(())
    }

    pub(super) fn step_castle_unpack(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let mut due: Vec<Entity> = tick
            .state()
            .store(ids.pending_unpack)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, pending)| pending.due(now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        due.sort_unstable_by_key(|entity| entity.index());
        for castle in due {
            tick.state_mut().remove(ids.pending_unpack, castle);
            if tick.state().is_live(castle) {
                self.unpack_castle(tick, ids, castle)?;
            }
        }
        Ok(())
    }

    pub(super) fn unpack_castle(&self, tick: &mut Tick<'_>, ids: Ids, site: Entity) -> Result<()> {
        let world = tick.state();
        let (Some(&castle_kind), Some(owner), Some(body)) = (
            world.get(ids.kind, site),
            world.get(ids.owner, site).map(|owner| owner.0),
            world.get(ids.body, site),
        ) else {
            return Ok(());
        };
        let (origin, facing) = (body.0.position, body.0.facing);
        let Some(castle) = self
            .kinds
            .get(castle_kind)
            .and_then(|entry| entry.record.castle.as_ref())
            .filter(|castle| castle.instant_unpack)
        else {
            return Ok(());
        };
        let side = self
            .player_sides()
            .iter()
            .find(|(player, _)| *player == owner)
            .map_or("", |(_, side)| side.as_str());
        let Some((name, layout)) = crate::castle::layout_for(&self.castle_layouts, castle, side)
        else {
            diag!(
                "castle: entity {} (side {side:?}) has no loaded layout to unpack",
                site.index()
            );
            return Ok(());
        };
        let plan = crate::castle::unpack_plan(&self.kinds, castle, layout, origin, facing);
        let neutral = self.neutral_player();
        let mut placed = 0usize;
        let mut keep = None;
        for entry in plan {
            let Some(member_owner) = (if entry.owned { Some(owner) } else { neutral }) else {
                continue;
            };
            let Some(member) = self.spawn_kind_placed(
                tick,
                ids,
                entry.kind,
                Placement::Exact(entry.at),
                entry.facing,
                member_owner,
            )?
            else {
                continue;
            };
            tick.state_mut().insert(
                ids.castle_member,
                member,
                CastleMember {
                    castle: site,
                    castle_kind,
                },
            )?;
            placed = placed.saturating_add(1);
            if keep.is_none()
                && self
                    .kinds
                    .get(entry.kind)
                    .is_some_and(|found| found.record.has_kind("CASTLE_KEEP"))
            {
                keep = Some(member);
            }
        }
        if let Some(keep) = keep {
            if let Some(name) = tick.state_mut().remove(ids.script_name, site) {
                tick.state_mut().insert(ids.script_name, keep, name)?;
            }
        }
        diag!(
            "castle: entity {} of player {} unpacked {name}: {placed} object(s) placed",
            site.index(),
            owner.get()
        );
        Ok(())
    }

    pub(super) fn castle_members_of(&self, world: &World, ids: Ids, castle: Entity) -> Vec<Entity> {
        world
            .store(ids.castle_member)
            .map(|store| {
                store
                    .iter()
                    .filter(|(member, link)| link.castle == castle && world.is_live(*member))
                    .map(|(member, _)| member)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn castle_deaths(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[(Entity, f32)],
    ) -> Result<Vec<(Entity, f32)>> {
        let world = tick.state();
        let mut castles: Vec<Entity> = killed
            .iter()
            .filter_map(|(entity, _)| {
                let member = *world.get(ids.castle_member, *entity)?;
                let is_keep = world
                    .get(ids.kind, *entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| entry.record.has_kind("CASTLE_KEEP"));
                let kills_everything = self
                    .kinds
                    .get(member.castle_kind)
                    .and_then(|entry| entry.record.castle.as_ref())
                    .is_some_and(|castle| castle.keep_death_kills_everything);
                (is_keep && kills_everything).then_some(member.castle)
            })
            .collect();
        if castles.is_empty() {
            return Ok(Vec::new());
        }
        castles.sort_unstable_by_key(|castle| castle.index());
        castles.dedup();
        let mut victims: Vec<Entity> = world
            .store(ids.castle_member)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, member)| castles.contains(&member.castle))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        victims.extend(castles.iter().copied());
        victims.sort_unstable_by_key(|entity| entity.index());
        victims.dedup();
        victims.retain(|entity| {
            world.is_live(*entity)
                && !world.contains(ids.doomed, *entity)
                && !killed.iter().any(|(dead, _)| dead == entity)
        });
        for &victim in &victims {
            if let Some(mut health) = tick.state().get(ids.health, victim).copied() {
                if !health.0.kill() {
                    health.0.set_current(0.0);
                }
                tick.state_mut().insert(ids.health, victim, health)?;
            }
        }
        if !victims.is_empty() {
            diag!(
                "castle: keep died - {} castle object(s) die with it (KeepDeathKillsEverything)",
                victims.len()
            );
        }
        Ok(victims.into_iter().map(|entity| (entity, 0.0)).collect())
    }

    pub(crate) fn foundation_target(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        pad: Entity,
        button: u8,
    ) -> core::result::Result<Kind, &'static str> {
        if world.get(ids.owner, pad).map(|owner| owner.0) != Some(player) {
            return Err("not the builder's");
        }
        if !world.is_live(pad) || world.contains(ids.doomed, pad) {
            return Err("dead");
        }
        let Some(entry) = world
            .get(ids.kind, pad)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return Err("no template");
        };
        if entry.record.foundation.is_none() {
            return Err("not a build plot");
        }
        if pad_is_occupied(world, ids.pad_occupant, ids.doomed, pad) {
            return Err("already built on");
        }
        let Some(&target) = entry.foundation_builds.get(usize::from(button)) else {
            return Err("no such button");
        };
        let needs_met = entry
            .foundation_needs
            .get(usize::from(button))
            .is_none_or(|needs| {
                needs.met(
                    &self.effective_upgrades_in(world, ids, pad),
                    &self.upgrade_bits,
                )
            });
        if !needs_met {
            return Err("its NeededUpgrade is not held");
        }
        let Some(record) = self.kinds.get(target).map(|entry| &entry.record) else {
            return Err("no template");
        };
        let Some((cost, _)) = self.price_for(world, ids, player, record) else {
            return Err("no usable BuildCost/BuildTime");
        };
        let economy = Self::economy_ids(ids);
        if !crate::economy::purse_of(world, economy, player).can_afford(cost) {
            return Err("not enough resources");
        }
        Ok(target)
    }

    pub(super) fn foundation_construct(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        button: u8,
    ) -> Result<()> {
        for pad in self.selection(tick, ids, player) {
            let target = match self.foundation_target(tick.state(), ids, player, pad, button) {
                Ok(target) => target,
                Err(why) => {
                    diag!(
                        "foundation: pad {} button {} refused - {why}",
                        pad.index(),
                        u16::from(button).saturating_add(1)
                    );
                    continue;
                }
            };
            return self.build_on_pad(tick, ids, player, pad, target);
        }
        Ok(())
    }

    fn build_on_pad(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        pad: Entity,
        target: Kind,
    ) -> Result<()> {
        let Some(entry) = self.kinds.get(target) else {
            return Ok(());
        };
        let record = &entry.record;
        let (Some((cost, frames)), Some(body)) = (
            self.price_for(tick.state(), ids, player, record),
            tick.state().get(ids.body, pad).copied(),
        ) else {
            return Ok(());
        };
        let (position, facing) = (body.0.position, body.0.facing);
        let Some(mut spawn) = self
            .kinds
            .spawn(&self.combat, target, position, facing, player)
        else {
            return Ok(());
        };
        let economy = Self::economy_ids(ids);
        let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
        else {
            return Ok(());
        };
        let mut purse = crate::economy::purse_of(tick.state(), economy, player);
        purse.withdraw(cost);
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        crate::match_stats::record(tick, ids.match_stats, player_entity, |stats| {
            stats.spend(crate::match_stats::SpendKind::Structures, cost);
        });
        spawn.producer = None;
        spawn.income = None;
        spawn.upkeep = None;
        spawn.weapon = None;
        spawn.vision = Some(spawn.shape.radius.max(1.0));
        let site = tick.state_mut().spawn()?;
        self.insert_spawn(tick, ids, site, &spawn, &self.combat)?;
        self.apply_weather_to_new_object(tick, ids, site)?;
        if let Some(mut health) = tick.state().get(ids.health, site).copied() {
            health.0.set_current(1.0);
            tick.state_mut().insert(ids.health, site, health)?;
        }
        let footprint = footprint_of(record);
        if !footprint.is_empty() {
            self.stamp_footprint(tick, ids, site, position, facing.radians(), footprint)?;
        }
        let worker_kind = self.getting_built_worker(record, player);
        let worker = self.spawn_site_worker(tick, ids, worker_kind, site, player)?;
        let created = tick.frame();
        tick.state_mut().insert(
            ids.construction,
            site,
            Construction {
                builder: None,
                elapsed: 0,
                rebuild: None,
                worker,
                delay: 0,
                created,
            },
        )?;
        tick.state_mut()
            .insert(ids.pad_occupant, pad, PadOccupant(site))?;
        if let Some(castle) = tick.state().get(ids.castle_member, pad).copied() {
            tick.state_mut().insert(ids.castle_member, site, castle)?;
        }
        tick.state_mut().remove(ids.selected, pad);
        let variation = tick
            .state()
            .get(ids.kind, pad)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.foundation)
            .and_then(|foundation| foundation.build_variation_condition());
        if let Some(token) = variation {
            self.apply_effects(
                tick,
                ids,
                vec![(
                    site,
                    Effect::ModelConditions {
                        add: std::sync::Arc::from([std::sync::Arc::<str>::from(token)]),
                        remove: std::sync::Arc::from([]),
                    },
                )],
            )?;
        }
        diag!(
            "foundation: {} ordered on pad {} by player {} - {cost} charged, {frames} frame(s) to build",
            record.name,
            pad.index(),
            player.get()
        );
        Ok(())
    }

    pub(crate) fn pad_is_occupied(&self, world: &World, ids: Ids, pad: Entity) -> bool {
        pad_is_occupied(world, ids.pad_occupant, ids.doomed, pad)
    }

    pub(super) fn castle_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        granter: Entity,
        upgrade: &str,
        wall_radius: Option<f32>,
    ) -> Result<()> {
        let Some(bit) = self.upgrade_bits.bit(upgrade) else {
            diag!("castle: {upgrade} is not an Upgrade - nothing handed out");
            return Ok(());
        };
        let world = tick.state();
        let castle = world
            .get(ids.castle_member, granter)
            .map(|member| member.castle);
        let mut targets: Vec<Entity> = castle
            .and_then(|castle| {
                world.store(ids.castle_member).map(|store| {
                    store
                        .iter()
                        .filter(|(_, member)| member.castle == castle)
                        .map(|(entity, _)| entity)
                        .collect::<Vec<Entity>>()
                })
            })
            .unwrap_or_default();
        if let Some(radius) = wall_radius {
            targets.extend(self.walls_within(world, ids, granter, radius));
        }
        targets.retain(|entity| world.is_live(*entity) && !world.contains(ids.doomed, *entity));
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        for target in &targets {
            self.grant_object_upgrade(tick, ids, *target, bit)?;
        }
        if let Some(castle) = castle {
            let mut held = tick
                .state()
                .get(ids.castle_upgrades, castle)
                .cloned()
                .unwrap_or_default();
            held.0.set(bit);
            tick.state_mut().insert(ids.castle_upgrades, castle, held)?;
        }
        diag!(
            "castle: {upgrade} handed to {} castle object(s)",
            targets.len()
        );
        Ok(())
    }

    fn walls_within(&self, world: &World, ids: Ids, granter: Entity, radius: f32) -> Vec<Entity> {
        let (Some(centre), Some(owner)) = (
            world.get(ids.body, granter).map(|body| body.0.position),
            world.get(ids.owner, granter).map(|owner| owner.0),
        ) else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.kind) else {
            return Vec::new();
        };
        let reach = radius * radius;
        store
            .iter()
            .filter(|(entity, _)| world.get(ids.owner, *entity).map(|o| o.0) == Some(owner))
            .filter(|(_, kind)| {
                self.kinds
                    .get(**kind)
                    .is_some_and(|entry| WALL_KINDS.iter().any(|name| entry.record.has_kind(name)))
            })
            .filter(|(entity, _)| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .map(|(entity, _)| entity)
            .collect()
    }

    pub(super) fn inherit_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        upgrade: &str,
        radius: f32,
        filter: &crate::defeat::ObjectFilter,
    ) -> Result<()> {
        let Some(bit) = self.upgrade_bits.bit(upgrade) else {
            diag!("castle: {upgrade} is not an Upgrade - nothing inherited");
            return Ok(());
        };
        let world = tick.state();
        if world
            .get(ids.object_upgrades, entity)
            .is_some_and(|held| held.0.contains(bit))
        {
            return Ok(());
        }
        let (Some(centre), Some(owner), Some(store)) = (
            world.get(ids.body, entity).map(|body| body.0.position),
            world.get(ids.owner, entity).map(|owner| owner.0),
            world.store(ids.kind),
        ) else {
            return Ok(());
        };
        let reach = radius * radius;
        let found = store.iter().any(|(other, kind)| {
            other != entity
                && world.get(ids.owner, other).map(|o| o.0) == Some(owner)
                && world
                    .get(ids.object_upgrades, other)
                    .is_some_and(|held| held.0.contains(bit))
                && self
                    .kinds
                    .get(*kind)
                    .is_some_and(|entry| filter.passes(&entry.record.kind_of, &entry.record.name))
                && world.get(ids.body, other).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
        });
        if found {
            self.grant_object_upgrade(tick, ids, entity, bit)?;
            diag!("castle: entity {} inherited {upgrade}", entity.index());
        }
        Ok(())
    }

    pub(super) fn inherit_castle_upgrades(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
    ) -> Result<()> {
        let held = tick
            .state()
            .get(ids.castle_member, member)
            .map(|link| link.castle)
            .and_then(|castle| tick.state().get(ids.castle_upgrades, castle))
            .map(|held| held.0.clone());
        let Some(held) = held.filter(|held| !held.is_empty()) else {
            return Ok(());
        };
        self.grant_object_upgrades(tick, ids, member, &held)
    }
}

const WALL_KINDS: [&str; 4] = ["WALL_UPGRADE", "WALL_HUB", "WALL_SEGMENT", "DEFENSIVE_WALL"];
