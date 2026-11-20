use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_campaign::walk_on_anchor;
use mint_sage_combat::ModifierList;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::map_scripts::orders::ScriptOrder;
use super::{Ids, RotwkRules};
use crate::diag::diag;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrCarry {
    pub(crate) skill_points: f32,
    pub(crate) rank_points: i32,
    pub(crate) sciences: Vec<String>,
    pub(crate) upgrades: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrStart {
    pub(crate) seats: Vec<WotrSeat>,
    pub(crate) skill_scalars: Vec<f32>,
    pub(crate) revival_multipliers: (f32, f32),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrSeat {
    pub(crate) player: PlayerId,
    pub(crate) armies: Vec<WotrArmy>,
    pub(crate) structures: Vec<WotrStructure>,
    pub(crate) modifiers: Option<ModifierList>,
    pub(crate) purchase_points: u32,
    pub(crate) skill_points: f32,
    pub(crate) sciences: Vec<String>,
    pub(crate) upgrades: Vec<String>,
    pub(crate) fallen: Vec<(String, u32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrArmy {
    pub(crate) army: u32,
    pub(crate) units: Vec<(String, u32, u32)>,
    pub(crate) arrives: u32,
    pub(crate) stand: [f32; 3],
    pub(crate) face: [f32; 3],
    pub(crate) walk_to: Option<Vec2>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WotrStructure {
    pub(crate) template: String,
    pub(crate) army: u32,
    pub(crate) at: Vec2,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct WotrRun {
    pub(crate) arrived: Vec<bool>,
    pub(crate) members: Vec<(Entity, u32)>,
    pub(crate) start_ranks: Vec<(PlayerId, u16)>,
    pub(crate) fallen: Vec<(PlayerId, crate::unit::Kind, u32)>,
    pub(crate) kills: Vec<(Entity, u32)>,
    pub(crate) revive_prices: Vec<(PlayerId, crate::unit::Kind, i64, f32)>,
}

impl WotrRun {
    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.arrived.len());
        for arrived in &self.arrived {
            hasher.write_bool(*arrived);
        }
        hasher.write_usize(self.members.len());
        for (entity, army) in &self.members {
            entity.hash_into(hasher);
            hasher.write_u32(*army);
        }
        for (player, rank) in &self.start_ranks {
            hasher.write_u8(player.get());
            hasher.write_u16(*rank);
        }
        hasher.write_usize(self.fallen.len());
        for (player, kind, army) in &self.fallen {
            hasher.write_u8(player.get());
            hasher.write_u16(kind.0);
            hasher.write_u32(*army);
        }
        hasher.write_usize(self.kills.len());
        for (entity, kills) in &self.kills {
            entity.hash_into(hasher);
            hasher.write_u32(*kills);
        }
        hasher.write_usize(self.revive_prices.len());
        for (player, kind, cost, seconds) in &self.revive_prices {
            hasher.write_u8(player.get());
            hasher.write_u16(kind.0);
            hasher.write_i64(*cost);
            hasher.write_u32(seconds.to_bits());
        }
    }

    pub(crate) fn revive_price(
        &self,
        player: PlayerId,
        kind: crate::unit::Kind,
    ) -> Option<(i64, f32)> {
        self.revive_prices
            .iter()
            .find(|(p, k, _, _)| (*p, *k) == (player, kind))
            .map(|(_, _, cost, seconds)| (*cost, *seconds))
    }

    pub(crate) fn kills_of(&self, entity: Entity) -> u32 {
        self.kills
            .iter()
            .find(|(e, _)| *e == entity)
            .map_or(0, |(_, kills)| *kills)
    }

    pub(crate) fn army_of(&self, entity: Entity) -> Option<u32> {
        self.members
            .iter()
            .find(|(held, _)| *held == entity)
            .map(|(_, army)| *army)
    }
}

impl RotwkRules {
    fn wotr_start(&self) -> Option<Arc<WotrStart>> {
        self.map_scripts
            .as_ref()
            .and_then(|engine| engine.wotr_start())
            .map(Arc::clone)
    }

    pub(super) fn note_wotr_kill(&self, tick: &mut Tick<'_>, ids: Ids, killer: Entity) {
        let horde = tick
            .state()
            .get(ids.member, killer)
            .map(|member| member.horde);
        let Some(run) = Self::wotr_run_mut(tick, ids) else {
            return;
        };
        for entity in std::iter::once(killer).chain(horde) {
            match run.kills.iter_mut().find(|(e, _)| *e == entity) {
                Some((_, kills)) => *kills = kills.saturating_add(1),
                None => run.kills.push((entity, 1)),
            }
        }
    }

    fn wotr_run_mut<'t>(tick: &'t mut Tick<'_>, ids: Ids) -> Option<&'t mut WotrRun> {
        let (_, state) = tick
            .state_mut()
            .store_mut(ids.map_script_state)?
            .iter_mut()
            .next()?;
        state.wotr.as_mut()
    }

    pub(crate) fn wotr_run<'w>(&self, world: &'w World) -> Option<&'w WotrRun> {
        let ids = self.ids()?;
        let (_, state) = world.store(ids.map_script_state)?.iter().next()?;
        state.wotr.as_ref()
    }

    fn restore_wotr_player(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        seat: &WotrSeat,
    ) -> Result<Option<u16>> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), seat.player)
        else {
            return Ok(None);
        };
        let ranks = self.ranks_of(seat.player);
        let Some(science) = tick.state_mut().get_mut(ids.player_science, entity) else {
            return Ok(None);
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let carried = seat.skill_points.max(0.0) as u32;
        let mut granted = science.add_skill_points(carried, ranks);
        for name in &seat.sciences {
            if let Some(id) = self.sciences.id(name) {
                if science.add(id) {
                    granted.push(id);
                }
            }
        }
        science.purchase_points = science.purchase_points.saturating_add(seat.purchase_points);
        let rank = science.rank;
        let mask = self.upgrade_bits.mask(&seat.upgrades);
        if !mask.is_empty() {
            let held = tick
                .state()
                .get(ids.player_upgrades, entity)
                .map_or_else(Default::default, |held| held.0.clone());
            tick.state_mut().insert(
                ids.player_upgrades,
                entity,
                crate::economy::PlayerUpgrades(held.union(&mask)),
            )?;
        }
        self.sciences_acquired(tick, ids, seat.player, &granted)?;
        Ok(Some(rank))
    }

    pub(crate) fn wotr_player_carry(&self, world: &World, player: PlayerId) -> Option<WotrCarry> {
        let ids = self.ids()?;
        let rank_points = crate::economy::player_entity(world, Self::economy_ids(ids), player)
            .and_then(|entity| world.get(ids.player_science, entity))
            .map_or(0, |state| self.ranks_of(player).granted_through(state.rank));
        let capture = self.player_capture(world, ids, player)?;
        let points = capture.skill_points.max(0.0);
        let sciences = capture
            .sciences
            .into_iter()
            .filter(|science| !science.grantable)
            .map(|science| science.name)
            .collect();
        let upgrades = capture
            .upgrades
            .into_iter()
            .filter(|upgrade| upgrade.persists)
            .map(|upgrade| upgrade.name)
            .collect();
        Some(WotrCarry {
            skill_points: points,
            rank_points: i32::try_from(rank_points).unwrap_or(i32::MAX),
            sciences,
            upgrades,
        })
    }

    pub(super) fn start_wotr_battle(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(start) = self.wotr_start() else {
            return Ok(());
        };
        let mut ranks = Vec::new();
        for seat in &start.seats {
            if let Some(rank) = self.restore_wotr_player(tick, ids, seat)? {
                ranks.push((seat.player, rank));
            }
        }
        if let Some(run) = Self::wotr_run_mut(tick, ids) {
            run.start_ranks = ranks;
        }
        for seat in &start.seats {
            self.seed_revivals(tick, ids, seat, start.revival_multipliers)?;
        }
        for seat in &start.seats {
            for structure in &seat.structures {
                let Some(kind) = self.kinds.kind(&structure.template) else {
                    diag!("wotr: no template {} for a plot", structure.template);
                    continue;
                };
                if let Some(entity) =
                    self.place_structure_complete(tick, ids, seat.player, kind, structure.at, 0.0)?
                {
                    self.join_army(tick, ids, entity, structure.army);
                    diag!(
                        "wotr: {} placed complete at ({:.0}, {:.0}) for player {}, army {}",
                        structure.template,
                        structure.at.x,
                        structure.at.y,
                        seat.player.get(),
                        structure.army
                    );
                }
            }
        }
        self.step_wotr_battle(tick, ids)
    }

    fn seed_revivals(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        seat: &WotrSeat,
        multipliers: (f32, f32),
    ) -> Result<()> {
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), seat.player)
        else {
            return Ok(());
        };
        for (name, level, experience) in &seat.fallen {
            let Some(kind) = self.kinds.kind(name) else {
                diag!("wotr: fallen hero {name:?} has no template in this match");
                continue;
            };
            let slot = self
                .roster_of(tick.state(), ids, seat.player)
                .and_then(|roster| {
                    roster.0.iter().position(|slot| {
                        slot.kind == Some(kind) && slot.state == crate::heroes::HeroState::Available
                    })
                });
            let Some(slot) = slot else {
                diag!(
                    "wotr: fallen hero {name} is not available on player {}'s hero page",
                    seat.player.get()
                );
                continue;
            };
            let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
                continue;
            };
            let (cost, seconds) = crate::heroes::campaign_revive_price(
                &record.respawn,
                &record.respawn_entries,
                *level,
                multipliers,
            );
            self.set_hero_state(
                tick,
                ids,
                player,
                slot,
                crate::heroes::HeroState::Dead {
                    points: *experience,
                },
            )?;
            if let Some(run) = Self::wotr_run_mut(tick, ids) {
                run.revive_prices.push((seat.player, kind, cost, seconds));
            }
            diag!(
                "wotr: fallen hero {name} (level {level}) in player {}'s revive list at {cost} for {seconds}s",
                seat.player.get()
            );
        }
        Ok(())
    }

    pub(super) fn step_wotr_battle(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(start) = self.wotr_start() else {
            return Ok(());
        };
        let now = tick.frame().get();
        let mut index = 0usize;
        for seat in &start.seats {
            for army in &seat.armies {
                let due = u64::from(army.arrives) <= now;
                let arrived = Self::wotr_run_mut(tick, ids)
                    .and_then(|run| run.arrived.get(index).copied())
                    .unwrap_or(true);
                if due && !arrived {
                    if let Some(slot) =
                        Self::wotr_run_mut(tick, ids).and_then(|run| run.arrived.get_mut(index))
                    {
                        *slot = true;
                    }
                    self.summon_army(tick, ids, seat, army)?;
                }
                index = index.saturating_add(1);
            }
        }
        Ok(())
    }

    fn summon_army(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        seat: &WotrSeat,
        army: &WotrArmy,
    ) -> Result<()> {
        let mut instances = Vec::new();
        for (name, count, level) in &army.units {
            let Some(kind) = self.kinds.kind(name) else {
                diag!("wotr: {name:?} has no template in this match - not summoned");
                continue;
            };
            for _ in 0..(*count).max(1) {
                instances.push((kind, *level));
            }
        }
        let anchor = walk_on_anchor(Some(army.stand), Some(army.face));
        let facing = Facing::from_radians(anchor.angle);
        let origin = Vec2::new(anchor.start[0], anchor.start[1]);
        let kinds: Vec<crate::unit::Kind> = instances.iter().map(|(kind, _)| *kind).collect();
        let places = self.walk_on_places(&kinds, origin, facing);
        let mut leaders = Vec::new();
        for ((kind, level), at) in instances.iter().zip(places) {
            let made = self.spawn_template(tick, ids, seat.player, *kind, at, facing)?;
            for entity in &made {
                self.join_army(tick, ids, *entity, army.army);
                self.recompute_bonuses(tick, ids, *entity)?;
            }
            let Some(&container) = made.last() else {
                continue;
            };
            if *level > 1 {
                self.gain_levels(tick, ids, container, level.saturating_sub(1), None)?;
            }
            leaders.push(container);
        }
        diag!(
            "wotr: army {} of player {} summoned on tick {}: {} object(s) at ({:.0}, {:.0})",
            army.army,
            seat.player.get(),
            tick.frame().get(),
            leaders.len(),
            origin.x,
            origin.y
        );
        if let Some(goal) = army.walk_to {
            self.issue_order_to(tick, ids, &leaders, ScriptOrder::Move(goal, false))?;
        }
        Ok(())
    }

    fn join_army(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity, army: u32) {
        if let Some(run) = Self::wotr_run_mut(tick, ids) {
            if run.army_of(entity).is_none() {
                run.members.push((entity, army));
            }
        }
    }

    pub(super) fn note_wotr_fallen(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        roster_owner: Entity,
        hero: Entity,
        kind: crate::unit::Kind,
    ) {
        let Some(owner) = tick.state().get(ids.slot, roster_owner).map(|slot| slot.0) else {
            return;
        };
        if let Some(run) = Self::wotr_run_mut(tick, ids) {
            if let Some(army) = run.army_of(hero) {
                run.fallen.retain(|(p, k, _)| (*p, *k) != (owner, kind));
                run.fallen.push((owner, kind, army));
            }
        }
    }

    pub(super) fn inherit_army(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        producer: Entity,
        produced: Entity,
    ) {
        let identity = tick
            .state()
            .get(ids.owner, produced)
            .map(|owner| owner.0)
            .zip(tick.state().get(ids.kind, produced).copied());
        let army = Self::wotr_run_mut(tick, ids).and_then(|run| {
            if let Some((owner, kind)) = identity {
                run.revive_prices
                    .retain(|(p, k, _, _)| (*p, *k) != (owner, kind));
            }
            let own = identity.and_then(|(owner, kind)| {
                let at = run
                    .fallen
                    .iter()
                    .position(|(p, k, _)| (*p, *k) == (owner, kind))?;
                Some(run.fallen.remove(at).2)
            });
            own.or_else(|| run.army_of(producer))
        });
        if let Some(army) = army {
            self.join_army(tick, ids, produced, army);
        }
    }

    pub(super) fn wotr_player_list(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<ModifierList> {
        let engine = self.map_scripts.as_ref()?;
        let start = engine.wotr_start()?;
        let owner = world.get(ids.owner, entity)?.0;
        let record = &self.kinds.get(*world.get(ids.kind, entity)?)?.record;
        if record.has_kind("HERO") || record.has_kind("STRUCTURE") {
            return None;
        }
        start
            .seats
            .iter()
            .find(|seat| seat.player == owner)
            .and_then(|seat| seat.modifiers.clone())
    }

    pub(crate) fn army_summary_object(
        &self,
        world: &World,
        entity: Entity,
    ) -> Option<(String, u32, u32, bool)> {
        let ids = self.ids()?;
        if !world.is_live(entity) || world.contains(ids.doomed, entity) {
            return None;
        }
        let record = &self.kinds.get(*world.get(ids.kind, entity)?)?.record;
        let level = self.rank_of(world, entity).map_or(1, u32::from).max(1);
        let experience = self.experience_of(world, ids, entity).map_or(0, |(_, p)| p);
        Some((
            record.name.clone(),
            level,
            experience,
            record.has_kind("ARMY_SUMMARY"),
        ))
    }

    pub(crate) fn defeated(&self, world: &World, player: PlayerId) -> bool {
        self.ids()
            .is_some_and(|ids| self.player_defeated(world, ids, player))
    }

    pub(crate) fn fallen_heroes(&self, world: &World, player: PlayerId) -> Vec<(String, u32, u32)> {
        let Some(roster) = self
            .ids()
            .and_then(|ids| self.roster_of(world, ids, player))
        else {
            return Vec::new();
        };
        roster
            .0
            .iter()
            .filter_map(|slot| {
                let (Some(kind), crate::heroes::HeroState::Dead { points }) =
                    (slot.kind, slot.state)
                else {
                    return None;
                };
                let record = &self.kinds.get(kind)?.record;
                Some((
                    record.name.clone(),
                    u32::from(self.rank_at(kind, points)),
                    points,
                ))
            })
            .collect()
    }

    pub(super) fn wotr_scaled_award(
        &self,
        world: &World,
        player: PlayerId,
        rank: u16,
        award: u32,
    ) -> u32 {
        let Some(start) = self.map_scripts.as_ref().and_then(|e| e.wotr_start()) else {
            return award;
        };
        let Some(&last) = start.skill_scalars.last() else {
            return award;
        };
        let at_start = self
            .wotr_run(world)
            .and_then(|run| run.start_ranks.iter().find(|(p, _)| *p == player))
            .map_or(rank, |(_, r)| *r);
        let gained = usize::from(rank.saturating_sub(at_start));
        let scalar = start.skill_scalars.get(gained).copied().unwrap_or(last);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let scaled = (award as f32 * scalar) as u32;
        scaled
    }
}
