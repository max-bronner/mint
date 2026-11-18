use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_campaign::{
    walk_on_anchor, CapturedObject, Carryover, PlayerCapture, ScienceFact, UpgradeFact,
};
use mint_sage_formation::{build_drag, rotate_drag, DragUnit};
use mint_sage_save::CarryoverUnit;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::economy::PlayerUpgrades;
use crate::game::LOCAL_PLAYER;
use crate::match_values::{MatchResult, PlayerResult};
use crate::sciences::PlayerScience;

const WALK_ON_TEMPLATE: &str = "WalkOnTemplate";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CarryoverStart {
    pub(crate) carry: Carryover,
    pub(crate) delayed: Vec<String>,
    pub(crate) start: Option<[f32; 3]>,
    pub(crate) rally: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mark {
    pub(crate) entity: Entity,
    pub(crate) missions: i32,
    pub(crate) kills_total: i32,
    pub(crate) kills_mission: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CarryoverRun {
    pub(crate) carry: Carryover,
    pub(crate) marks: Vec<Mark>,
}

impl CarryoverRun {
    pub(crate) fn new(carry: Carryover) -> Self {
        Self {
            carry,
            marks: Vec::new(),
        }
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.carry.available().len());
        for record in self.carry.available().iter().chain(self.carry.used()) {
            hasher.write_bytes(&record.template_name.0);
            hasher.write_i32(record.count);
            hasher.write_f32(record.experience);
        }
        hasher.write_f32(self.carry.skill_points());
        hasher.write_f32(self.carry.cash_bounty());
        hasher.write_usize(self.marks.len());
        for mark in &self.marks {
            mark.entity.hash_into(hasher);
            hasher.write_i32(mark.missions);
            hasher.write_i32(mark.kills_total);
            hasher.write_i32(mark.kills_mission);
        }
    }
}

fn state_entity(world: &World, ids: Ids) -> Option<Entity> {
    world
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next())
        .map(|(entity, _)| entity)
}

impl RotwkRules {
    fn carryover_run_mut<'t>(
        tick: &'t mut Tick<'_>,
        ids: Ids,
    ) -> Option<&'t mut Option<CarryoverRun>> {
        let (_, state) = tick
            .state_mut()
            .store_mut(ids.map_script_state)?
            .iter_mut()
            .next()?;
        Some(&mut state.carryover)
    }

    pub(super) fn restore_carryover(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(start) = self
            .map_scripts
            .as_ref()
            .and_then(|engine| engine.carryover_start())
            .map(Arc::clone)
        else {
            return Ok(());
        };
        let Some(mut run) = Self::carryover_run_mut(tick, ids).and_then(Option::take) else {
            return Ok(());
        };
        self.restore_player(tick, ids, &run.carry)?;
        let mut waiting: Vec<(CarryoverUnit, crate::unit::Kind)> = Vec::new();
        run.carry.restore(&start.delayed, |record| {
            let name = record.template_name.to_string_lossy();
            match self.kinds.kind(&name) {
                Some(kind) => {
                    waiting.push((record.clone(), kind));
                    1
                }
                None => {
                    diag!("carryover: {name:?} has no template in this match - not spawned");
                    0
                }
            }
        });
        diag!(
            "carryover: {} record(s) waiting, {} delayed by the mission, {} to spawn",
            run.carry.available().len() + waiting.len(),
            start.delayed.len(),
            waiting.len()
        );
        let anchor = walk_on_anchor(start.start, start.rally);
        let facing = Facing::from_radians(anchor.angle);
        let origin = Vec2::new(anchor.start[0], anchor.start[1]);
        let mut instances: Vec<(&CarryoverUnit, crate::unit::Kind)> = Vec::new();
        for (record, kind) in &waiting {
            for _ in 0..record.count.max(1) {
                instances.push((record, *kind));
            }
        }
        let kinds: Vec<crate::unit::Kind> = instances.iter().map(|(_, kind)| *kind).collect();
        let places = self.walk_on_places(&kinds, origin, facing);
        for ((record, kind), at) in instances.iter().zip(places) {
            let made = self.spawn_template(tick, ids, LOCAL_PLAYER, *kind, at, facing)?;
            let Some(&container) = made.last() else {
                continue;
            };
            run.marks.push(Mark {
                entity: container,
                missions: record.carried.missions,
                kills_total: record.carried.kills_total,
                kills_mission: 0,
            });
            self.restore_carried(tick, ids, container, record)?;
            diag!(
                "carryover: spawned {:?} at ({:.0}, {:.0}) experience {}",
                record.template_name.to_string_lossy(),
                at.x,
                at.y,
                record.experience
            );
        }
        if let Some(slot) = Self::carryover_run_mut(tick, ids) {
            *slot = Some(run);
        }
        Ok(())
    }

    pub(super) fn walk_on_places(
        &self,
        instances: &[crate::unit::Kind],
        origin: Vec2,
        facing: Facing,
    ) -> Vec<Vec2> {
        let mut places = vec![origin; instances.len()];
        let Some(template) = self
            .drag_formation
            .templates
            .iter()
            .find(|template| template.name.eq_ignore_ascii_case(WALK_ON_TEMPLATE))
        else {
            return places;
        };
        let units: Vec<DragUnit> = instances
            .iter()
            .map(|kind| {
                let record = self.kinds.get(*kind).map(|entry| &entry.record);
                let class = record
                    .and_then(|record| self.drag_formation.classify(&record.kind_of, &record.name));
                let (width, depth) =
                    record.map_or((1.0, 1.0), super::drag_formation::drag_footprint);
                DragUnit {
                    offset: Vec2::new(0.0, 0.0),
                    class,
                    width,
                    depth,
                    template: u64::from(kind.0),
                }
            })
            .collect();
        for slot in build_drag(
            template,
            &units,
            self.drag_formation.row_padding,
            self.drag_formation.column_padding,
        ) {
            if let Some(place) = places.get_mut(slot.unit) {
                *place = origin.plus(rotate_drag(facing, slot.local));
            }
        }
        places
    }

    fn restore_carried(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        record: &CarryoverUnit,
    ) -> Result<()> {
        if record.experience > 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let points = record.experience as u32;
            self.restore_experience(tick, ids, entity, points)?;
        }
        let names = record.upgrades.0.iter().map(|name| name.to_string_lossy());
        let mask = self.upgrade_bits.mask(names);
        self.grant_object_upgrades(tick, ids, entity, &mask)
    }

    fn restore_player(&self, tick: &mut Tick<'_>, ids: Ids, carry: &Carryover) -> Result<()> {
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), LOCAL_PLAYER)
        else {
            return Ok(());
        };
        crate::spell_state::restore_bounty(
            tick.state_mut(),
            ids.cash_bounty,
            player_entity,
            carry.cash_bounty(),
        )?;
        let mask = self.upgrade_bits.mask(carry.upgrades());
        if !mask.is_empty() {
            let held = tick
                .state()
                .get(ids.player_upgrades, player_entity)
                .map_or_else(Default::default, |held| held.0.clone());
            tick.state_mut().insert(
                ids.player_upgrades,
                player_entity,
                PlayerUpgrades(held.union(&mask)),
            )?;
        }
        let mut granted = Vec::new();
        let held = tick
            .state()
            .get(ids.player_science, player_entity)
            .map(|held| (held.book, held.unavailable.clone()));
        let mut skill_points = 0;
        if let Some((book, unavailable)) = held {
            let ranks = self.ranks_of(LOCAL_PLAYER);
            let plan = self.spell_plans.get(usize::from(LOCAL_PLAYER.get()));
            let mut state = PlayerScience::start(
                plan.map_or(&[][..], |plan| plan.intrinsic.as_slice()),
                ranks,
                plan.and_then(|plan| plan.rank_cap),
                plan.map_or(0, |plan| plan.intrinsic_purchase_points),
            );
            state.book = book;
            state.unavailable = unavailable;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let points = carry.skill_points().max(0.0) as u32;
            granted.extend(state.add_skill_points(points, ranks));
            for name in carry.sciences() {
                if let Some(id) = self.sciences.id(name) {
                    if state.add(id) {
                        granted.push(id);
                    }
                }
            }
            skill_points = state.skill_points;
            tick.state_mut()
                .insert(ids.player_science, player_entity, state)?;
        }
        diag!(
            "carryover: player skill points {skill_points} (carried {}), cash bounty {}, upgrades [{}], {} of {} science(s) granted",
            carry.skill_points(),
            carry.cash_bounty(),
            carry.upgrades().join(" "),
            granted.len(),
            carry.sciences().len()
        );
        self.sciences_acquired(tick, ids, LOCAL_PLAYER, &granted)
    }

    pub(super) fn note_carryover_kill(&self, tick: &mut Tick<'_>, ids: Ids, killer: Entity) {
        let horde = tick
            .state()
            .get(ids.member, killer)
            .map(|member| member.horde);
        let Some(Some(run)) = Self::carryover_run_mut(tick, ids) else {
            return;
        };
        for mark in &mut run.marks {
            if mark.entity == killer || Some(mark.entity) == horde {
                mark.kills_total = mark.kills_total.saturating_add(1);
                mark.kills_mission = mark.kills_mission.saturating_add(1);
            }
        }
    }

    pub(crate) fn match_result(&self, world: &World) -> MatchResult {
        let Some(ids) = self.ids() else {
            return MatchResult::default();
        };
        let state =
            state_entity(world, ids).and_then(|anchor| world.get(ids.map_script_state, anchor));
        let run = state.and_then(|state| state.carryover.as_ref());
        MatchResult {
            end: state.and_then(|state| state.mission_end),
            roster: run.map(|run| run.carry.clone()),
            survivors: run
                .map(|run| {
                    run.marks
                        .iter()
                        .filter(|mark| {
                            world.is_live(mark.entity) && !world.contains(ids.doomed, mark.entity)
                        })
                        .filter_map(|mark| self.captured_object(world, ids, mark))
                        .collect()
                })
                .unwrap_or_default(),
            players: (0..self.player_count())
                .map(PlayerId::new)
                .map(|player| PlayerResult {
                    player,
                    fallen_heroes: self.revival_objects(world, ids, player),
                    state: self.player_capture(world, ids, player),
                })
                .collect(),
        }
    }

    fn captured_object(&self, world: &World, ids: Ids, mark: &Mark) -> Option<CapturedObject> {
        let kind = *world.get(ids.kind, mark.entity)?;
        let record = &self.kinds.get(kind)?.record;
        let (_, points) = self.experience_of(world, ids, mark.entity)?;
        let upgrades = world
            .get(ids.object_upgrades, mark.entity)
            .map(|held| self.upgrade_names(&held.0))
            .unwrap_or_default();
        Some(CapturedObject {
            template: record.name.clone(),
            hero: record.has_kind("HERO"),
            horde: record.has_kind("HORDE"),
            respawn_name: None,
            horde_name: None,
            experience: points as f32,
            level: self.rank_of(world, mark.entity).map_or(0, i32::from),
            upgrades,
            missions: mark.missions,
            kills_total: mark.kills_total,
            kills_mission: mark.kills_mission,
        })
    }

    pub(super) fn revival_objects(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> Vec<CapturedObject> {
        let Some(roster) = self.roster_of(world, ids, player) else {
            return Vec::new();
        };
        roster
            .0
            .iter()
            .filter_map(|slot| match (slot.kind, slot.state) {
                (Some(kind), crate::heroes::HeroState::Dead { points }) => {
                    let record = &self.kinds.get(kind)?.record;
                    Some(CapturedObject {
                        template: record.name.clone(),
                        hero: true,
                        experience: points as f32,
                        level: 1,
                        ..CapturedObject::default()
                    })
                }
                _ => None,
            })
            .collect()
    }

    fn upgrade_names(&self, mask: &mint_sim::UpgradeMask) -> Vec<String> {
        self.upgrade_bits
            .names()
            .filter(|(_, bit)| mask.contains(*bit))
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    pub(super) fn player_capture(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> Option<PlayerCapture> {
        let player = crate::economy::player_entity(world, Self::economy_ids(ids), player)?;
        #[allow(clippy::cast_precision_loss)]
        let skill_points = world
            .get(ids.player_science, player)
            .map_or(0.0, |state| state.skill_points as f32);
        let cash_bounty = world.get(ids.cash_bounty, player).map_or(0.0, |b| b.0);
        let sciences = world
            .get(ids.player_science, player)
            .map(|state| {
                state
                    .sciences
                    .iter()
                    .filter_map(|id| self.sciences.get(*id))
                    .map(|science| ScienceFact {
                        name: science.name.clone(),
                        grantable: science.grantable,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let upgrades = world
            .get(ids.player_upgrades, player)
            .map(|held| {
                self.upgrade_bits
                    .names()
                    .filter(|(_, bit)| held.0.contains(*bit))
                    .map(|(name, bit)| UpgradeFact {
                        name: name.to_owned(),
                        persists: self.upgrade_bits.persists_in_campaign(bit),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(PlayerCapture {
            skill_points,
            sciences,
            upgrades,
            cash_bounty,
        })
    }

    pub(super) fn create_delayed_carryover(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        run: &mut CarryoverRun,
        name: &str,
        owner: PlayerId,
        at: Vec2,
        angle: f32,
    ) -> Result<Option<Entity>> {
        let mut made: Option<(Entity, CarryoverUnit)> = None;
        let mut failure = None;
        let CarryoverRun { carry, .. } = run;
        carry.create_delayed(name, |record| {
            let Some(kind) = self.kinds.kind(&record.template_name.to_string_lossy()) else {
                return false;
            };
            match self.spawn_template(tick, ids, owner, kind, at, Facing::from_radians(angle)) {
                Ok(entities) => {
                    made = entities.last().map(|&entity| (entity, record.clone()));
                    made.is_some()
                }
                Err(error) => {
                    failure = Some(error);
                    false
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        let Some((entity, record)) = made else {
            diag!("carryover: delayed {name:?} - no waiting record spawned");
            return Ok(None);
        };
        diag!(
            "carryover: delayed {name:?} spawned at ({:.0}, {:.0}) experience {}",
            at.x,
            at.y,
            record.experience
        );
        run.marks.push(Mark {
            entity,
            missions: record.carried.missions,
            kills_total: record.carried.kills_total,
            kills_mission: 0,
        });
        self.restore_carried(tick, ids, entity, &record)?;
        Ok(Some(entity))
    }

    pub(super) fn revival_entry_from_carryover(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        run: &mut CarryoverRun,
        name: &str,
    ) -> Result<()> {
        let Some(kind) = self.kinds.kind(name) else {
            return Ok(());
        };
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), LOCAL_PLAYER)
        else {
            return Ok(());
        };
        let slot = self
            .roster_of(tick.state(), ids, LOCAL_PLAYER)
            .and_then(|roster| roster.0.iter().position(|slot| slot.kind == Some(kind)));
        let mut points = 0;
        let made = run.carry.create_delayed(name, |record| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let carried = record.experience.max(0.0) as u32;
            points = carried;
            slot.is_some()
        });
        if let (true, Some(slot)) = (made, slot) {
            self.set_hero_state(
                tick,
                ids,
                player,
                slot,
                crate::heroes::HeroState::Dead { points },
            )?;
        }
        Ok(())
    }
}
