use std::collections::BTreeMap;

use mint_movement::{Facing, Route, Vec2};
use mint_sage_modules::contain::{Offset, OpenContain, RingEntry};
use mint_sage_modules::effects::{names, Effect, Name};
use mint_sage_modules::object_filter::ObjectFilter;
use mint_sim::module::OwnerChange;
use mint_sim::{
    Component, Entity, Frame, ModuleFieldSource, PlayerId, Result, StateHasher, Tick, World,
};

use super::movement::PathBudgets;
use super::{Ids, IgnoredObstacle, RotwkRules};
use crate::diag::diag;

const ENTRY_REACH: f32 = 12.0;

const WAIT_TO_FORCE_MEMBER_TO_ENTER_DELAY: u32 =
    5 * (mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ);

const GRABBED: &str = "PASSENGER";

const HELD_ALOFT: &str = super::projectile::FREEFALL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContainStatus(u8);

impl ContainStatus {
    pub(crate) const UNSELECTABLE: Self = Self(1);
    pub(crate) const CAN_ATTACK: Self = Self(2);
    pub(crate) const ENCLOSED: Self = Self(4);
    pub(crate) const UNATTACKABLE: Self = Self(8);

    fn from_names(list: &[std::sync::Arc<str>]) -> Self {
        let mut bits = 0u8;
        for name in list {
            bits |= match () {
                () if name.eq_ignore_ascii_case("UNSELECTABLE") => Self::UNSELECTABLE.0,
                () if name.eq_ignore_ascii_case("CAN_ATTACK") => Self::CAN_ATTACK.0,
                () if name.eq_ignore_ascii_case("ENCLOSED") => Self::ENCLOSED.0,
                () if name.eq_ignore_ascii_case("UNATTACKABLE") => Self::UNATTACKABLE.0,
                () => 0,
            };
        }
        Self(bits)
    }

    pub(crate) const fn has(self, bit: Self) -> bool {
        self.0 & bit.0 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Passengers(pub(crate) Vec<Entity>, pub(crate) Option<PlayerId>);

impl Component for Passengers {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for entity in &self.0 {
            entity.hash_into(hasher);
        }
        hasher.write_u8(self.1.map_or(u8::MAX, |owner| owner.get()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Contained {
    pub(crate) container: Entity,
    pub(crate) status: ContainStatus,
    pub(crate) leaves_at: Frame,
    pub(crate) grabbed_at: Option<Frame>,
}

impl Component for Contained {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.container.hash_into(hasher);
        hasher.write_u8(self.status.0);
        self.leaves_at.hash_into(hasher);
        match self.grabbed_at {
            Some(at) => {
                hasher.write_u8(1);
                at.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rider {
    pub(crate) rider: Entity,
    pub(crate) container: Entity,
    pub(crate) slot: usize,
    pub(crate) prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PayloadFilled {
    pub(crate) initial: bool,
    pub(crate) triggers: u32,
}

impl Component for PayloadFilled {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.initial);
        hasher.write_u32(self.triggers);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entering {
    pub(crate) container: Entity,
    pub(crate) through: bool,
    pub(crate) since: Frame,
}

impl Component for Entering {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.container.hash_into(hasher);
        hasher.write_bool(self.through);
        self.since.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StragglerClock(Frame);

impl Component for StragglerClock {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy)]
struct Doorway {
    centre: Vec2,
    radius: f32,
    door: Vec2,
    inside: Vec2,
}

pub(crate) struct ContainView<'a> {
    pub(crate) open: &'a OpenContain,
    pub(crate) capacity: u32,
    pub(crate) cash_back: f32,
    pub(crate) ring: Option<&'a RingEntry>,
    pub(crate) garrison_family: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnterRefusal {
    NotAContainer,
    Full,
    Filtered,
    Relationship,
    Aircraft,
    WalkerStatus,
    Mounted,
    ReallyDamaged,
    ComboHorde,
}

fn in_frame(centre: Vec2, facing: Facing, offset: Offset) -> Vec2 {
    let (sin, cos) = mint_math::f32::sin_cos(facing.radians());
    Vec2::new(
        centre.x + offset.x * cos - offset.y * sin,
        centre.y + offset.x * sin + offset.y * cos,
    )
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ContainDamageReactions {
    evacuate_weapon: Vec<Option<mint_sage_combat::WeaponId>>,
    transport_rider_damage: Vec<bool>,
}

impl ContainDamageReactions {
    pub(crate) fn build(
        kinds: &crate::templates::KindTable,
        weapons: &mint_sage_combat::WeaponStore,
    ) -> Self {
        let bits = mint_sage_modules::UpgradeBits::default();
        let mut evacuate_weapon = Vec::with_capacity(kinds.len());
        let mut transport_rider_damage = Vec::with_capacity(kinds.len());
        for index in 0..kinds.len() {
            let entry = u16::try_from(index)
                .ok()
                .and_then(|k| kinds.get(crate::unit::Kind(k)));
            let modules = entry.map_or(&[][..], |entry| entry.record.declared_modules.as_slice());
            let evacuate = modules
                .iter()
                .rev()
                .find(|module| module.name.eq_ignore_ascii_case("EvacuateDamage"))
                .and_then(|module| {
                    let source = module.source(&bits);
                    let name = source.field("WeaponThatCausesEvacuation")?;
                    weapons.id(name)
                });
            evacuate_weapon.push(evacuate);
            transport_rider_damage.push(modules.iter().any(|module| {
                module
                    .name
                    .eq_ignore_ascii_case("HordeTransportContainDamage")
            }));
        }
        Self {
            evacuate_weapon,
            transport_rider_damage,
        }
    }

    fn evacuate_weapon(&self, kind: crate::unit::Kind) -> Option<mint_sage_combat::WeaponId> {
        self.evacuate_weapon
            .get(usize::from(kind.0))
            .copied()
            .flatten()
    }

    fn has_transport_rider_damage(&self, kind: crate::unit::Kind) -> bool {
        self.transport_rider_damage
            .get(usize::from(kind.0))
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "contain-damage: {} kind(s) with EvacuateDamage's weapon resolved, {} with HordeTransportContainDamage",
            self.evacuate_weapon.iter().filter(|w| w.is_some()).count(),
            self.transport_rider_damage.iter().filter(|b| **b).count(),
        )
    }
}

impl RotwkRules {
    pub(crate) fn contain_view<'a>(
        &self,
        world: &'a World,
        entity: Entity,
    ) -> Option<ContainView<'a>> {
        let wiring = self.modules.as_ref()?;
        wiring.contain_view(world, entity)
    }

    pub(crate) fn crew_speed_factor(&self, world: &World, ids: Ids, entity: Entity) -> f32 {
        let Some(wiring) = self.modules.as_ref() else {
            return 1.0;
        };
        let Some(module) = wiring.siege_contain(world, entity) else {
            return 1.0;
        };
        let crew = world.get(ids.passengers, entity).map_or(0, |list| {
            list.0
                .iter()
                .filter(|&&passenger| {
                    self.passes_passenger_filter(world, ids, module.crew_filter(), passenger)
                })
                .count()
        });
        #[allow(clippy::cast_precision_loss)]
        let crew = crew as f32;
        crew * module.speed_percent_per_crew()
    }

    pub(crate) fn contain_order_admits(
        &self,
        world: &World,
        player: PlayerId,
        container: Entity,
        object: Entity,
    ) -> bool {
        let Some(ids) = self.ids else { return false };
        if container == object {
            return false;
        }
        let subject = world
            .get(ids.member, object)
            .map_or(object, |member| member.horde);
        self.contain_admits(world, ids, player, container, subject)
            .is_ok()
    }

    pub(crate) fn ring_delivery_target(
        &self,
        world: &World,
        container: Entity,
        selection: &[Entity],
    ) -> bool {
        let Some(ids) = self.ids else { return false };
        if self.contain_view(world, container).is_none() {
            return false;
        }
        if !self.has_kind_bit(world, ids, container, "COMMANDCENTER") {
            return false;
        }
        selection
            .iter()
            .any(|&object| self.carries_status(world, object, "HOLDING_THE_RING"))
    }

    pub(crate) fn contain_report(&self, world: &World) -> Vec<String> {
        let Some(ids) = self.ids else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.passengers) else {
            return Vec::new();
        };
        store
            .iter()
            .map(|(container, list)| {
                let members: usize = list
                    .0
                    .iter()
                    .map(|passenger| {
                        world
                            .get(ids.horde, *passenger)
                            .map_or(1, |horde| horde.members.len())
                    })
                    .sum();
                format!(
                    "contain: entity {} holds {} passenger(s), {members} member(s)",
                    container.index(),
                    list.0.len()
                )
            })
            .collect()
    }

    pub(crate) fn contain_admits(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
        container: Entity,
        object: Entity,
    ) -> core::result::Result<(), EnterRefusal> {
        let view = self
            .contain_view(world, container)
            .ok_or(EnterRefusal::NotAContainer)?;
        if !view.open.enabled() {
            return Err(EnterRefusal::NotAContainer);
        }
        if let Some(ring) = view.ring {
            if self.ring_door_admits(world, ids, container, object, ring) {
                return Ok(());
            }
        }
        if self.walker_status_refuses(world, ids, object) {
            return Err(EnterRefusal::WalkerStatus);
        }
        if self.has_kind_bit(world, ids, object, "COMBO_HORDE")
            && !(self.has_kind_bit(world, ids, container, "GARRISON")
                || self.has_kind_bit(world, ids, container, "FS_CASH_PRODUCER"))
        {
            return Err(EnterRefusal::ComboHorde);
        }
        let judged = world
            .get(ids.horde, object)
            .and_then(|horde| horde.members.first().copied())
            .unwrap_or(object);
        let is_tunnel = self.is_tunnel(world, container);
        if is_tunnel && self.has_kind_bit(world, ids, judged, "AIRCRAFT") {
            return Err(EnterRefusal::Aircraft);
        }
        if !self.passes_passenger_filter(world, ids, view.open.passenger_filter(), object) {
            return Err(EnterRefusal::Filtered);
        }
        if view.open.passenger_filter().excludes("CAVALRY")
            && self.holds_condition_in(world, judged, "MOUNTED")
        {
            return Err(EnterRefusal::Mounted);
        }
        let can_enter_anything = self.carries_status(world, object, "CAN_ENTER_ANYTHING");
        if !can_enter_anything {
            let owner = world
                .get(ids.owner, container)
                .map_or(PlayerId::new(0), |held| held.0);
            let same = owner == player;
            let relation = self.object_victim_relation(world, object, container);
            let allied = matches!(relation, mint_sage_combat::victim::Relation::Allies);
            let hostile = matches!(relation, mint_sage_combat::victim::Relation::Enemies);
            let admitted = if same {
                view.open.allow_own_player_inside()
            } else if hostile {
                view.open.allow_enemies_inside()
            } else if allied {
                view.open.allow_allies_inside()
            } else {
                view.open.allow_neutral_inside()
            };
            if !admitted {
                return Err(EnterRefusal::Relationship);
            }
        }
        if view.garrison_family
            && self.really_damaged(world, ids, container)
            && !self.has_kind_bit(world, ids, container, "GARRISONABLE_UNTIL_DESTROYED")
        {
            return Err(EnterRefusal::ReallyDamaged);
        }
        if !can_enter_anything {
            let capacity = if is_tunnel {
                self.max_tunnel_capacity
            } else {
                view.capacity
            };
            let held = self.contain_count(world, ids, container);
            if held >= capacity {
                return Err(EnterRefusal::Full);
            }
        }
        Ok(())
    }

    fn walker_status_refuses(&self, world: &World, ids: Ids, object: Entity) -> bool {
        world.contains(ids.doomed, object)
            || world.contains(ids.defect_revert, object)
            || self.carries_status(world, object, "HOLDING_THE_RING")
    }

    fn ring_door_admits(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        object: Entity,
        ring: &RingEntry,
    ) -> bool {
        self.carries_status(world, object, ring.status())
            && world
                .get(ids.owner, container)
                .is_some_and(|held| Self::controller_of(world, ids, object) == Some(held.0))
            && self.doorway(world, ids, container).is_some()
    }

    fn really_damaged(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let Some(health) = world.get(ids.health, entity) else {
            return false;
        };
        let max = health.0.max();
        if max <= 0.0 {
            return false;
        }
        let fraction = health.0.current() / max;
        let thresholds = crate::combat::DamageThresholds::of(
            world
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record),
            self.damage_thresholds,
        );
        thresholds.rung(fraction) >= crate::combat::DamageRung::ReallyDamaged
    }

    fn passes_passenger_filter(
        &self,
        world: &World,
        ids: Ids,
        filter: &ObjectFilter,
        object: Entity,
    ) -> bool {
        let judged = world
            .get(ids.horde, object)
            .and_then(|horde| horde.members.first().copied())
            .unwrap_or(object);
        let Some(record) = world
            .get(ids.kind, judged)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return false;
        };
        filter.passes(&record.kind_of, &record.name)
    }

    fn contain_subjects(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> Vec<Entity> {
        let selected = self.selection(tick, ids, player);
        let owned: Vec<Entity> = selected
            .iter()
            .copied()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .collect();
        let mut out = crate::horde::hordes_of(tick.state(), Self::horde_ids(ids), &owned);
        out.extend(
            owned
                .iter()
                .copied()
                .filter(|entity| !tick.state().contains(ids.member, *entity)),
        );
        out.retain(|entity| !Self::emotion_refuses_order(tick.state(), ids, *entity));
        out.sort_unstable_by_key(|entity| entity.index());
        out.dedup();
        out
    }

    pub(super) fn enter_container(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        container: Entity,
    ) -> Result<()> {
        if !tick.state().is_live(container) {
            return Ok(());
        }
        let Some(entry) = self.contain_entry_point(tick.state(), ids, container) else {
            return Ok(());
        };
        let mut subjects = self.contain_subjects(tick, ids, player);
        subjects.retain(|subject| !self.siege_refuses_order(tick.state(), ids, *subject));
        let mut admitted = Vec::new();
        for subject in subjects {
            if self
                .contain_admits(tick.state(), ids, player, container, subject)
                .is_ok()
            {
                admitted.push(subject);
            }
        }
        if admitted.is_empty() {
            return Ok(());
        }
        self.move_to_exact_goal(tick, ids, player, entry)?;
        let since = tick.frame();
        for subject in &admitted {
            tick.state_mut().insert(
                ids.entering,
                *subject,
                Entering {
                    container,
                    through: false,
                    since,
                },
            )?;
        }
        diag!(
            "contain: {} object(s) ordered into entity {}",
            admitted.len(),
            container.index()
        );
        if let Some(&first) = admitted.first() {
            let kind = if self.ring_delivery_target(tick.state(), container, &[first]) {
                crate::voice::VoiceKind::DeliverRing
            } else {
                crate::voice::VoiceKind::Enter
            };
            self.emit_voice(tick, ids, first, kind, Some(container));
        }
        Ok(())
    }

    fn contain_entry_point(&self, world: &World, ids: Ids, container: Entity) -> Option<Vec2> {
        let view = self.contain_view(world, container)?;
        let body = world.get(ids.body, container)?;
        Some(in_frame(
            body.0.position,
            body.0.facing,
            view.open.entry_offset(),
        ))
    }

    pub(super) fn exit_container(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        passenger: Option<Entity>,
    ) -> Result<()> {
        if let Some(passenger) = passenger {
            return self.exit_one_passenger(tick, ids, player, passenger);
        }
        let selected = self.selection(tick, ids, player);
        let mut containers: Vec<Entity> = Vec::new();
        for entity in selected {
            if self.contain_view(tick.state(), entity).is_some() {
                containers.push(entity);
            }
            if let Some(held) = tick.state().get(ids.contained, entity).copied() {
                containers.push(held.container);
            }
        }
        containers.sort_unstable_by_key(|entity| entity.index());
        containers.dedup();
        for container in containers {
            let owned = tick
                .state()
                .get(ids.owner, container)
                .is_some_and(|owner| owner.0 == player);
            if !owned {
                continue;
            }
            self.schedule_exit(tick, ids, container)?;
        }
        Ok(())
    }

    fn exit_one_passenger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        passenger: Entity,
    ) -> Result<()> {
        let Some(mut held) = tick.state().get(ids.contained, passenger).copied() else {
            return Ok(());
        };
        let owned = tick
            .state()
            .get(ids.owner, held.container)
            .is_some_and(|owner| owner.0 == player);
        if !owned || held.leaves_at != Frame::FOREVER {
            return Ok(());
        }
        self.gather_tunnel_pool(tick, ids, held.container)?;
        held.leaves_at = tick.frame();
        let container = held.container;
        tick.state_mut().insert(ids.contained, passenger, held)?;
        diag!(
            "contain: entity {} alone leaving entity {}",
            passenger.index(),
            container.index()
        );
        Ok(())
    }

    pub(super) fn schedule_exit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
    ) -> Result<()> {
        self.gather_tunnel_pool(tick, ids, container)?;
        let Some(delay) = self
            .contain_view(tick.state(), container)
            .map(|view| view.open.exit_delay())
        else {
            return Ok(());
        };
        let Some(list) = tick.state().get(ids.passengers, container).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        for (ordinal, passenger) in list.0.iter().enumerate() {
            let Some(mut held) = tick.state().get(ids.contained, *passenger).copied() else {
                continue;
            };
            if held.leaves_at != Frame::FOREVER {
                continue;
            }
            let wait = delay
                .get()
                .saturating_mul(u32::try_from(ordinal).unwrap_or(u32::MAX));
            held.leaves_at = now
                .checked_add(mint_sim::LogicFrames::from_frames(wait))
                .unwrap_or(Frame::FOREVER);
            tick.state_mut().insert(ids.contained, *passenger, held)?;
        }
        diag!(
            "contain: entity {} emptying {} passenger(s)",
            container.index(),
            list.0.len()
        );
        Ok(())
    }

    pub(super) fn advance_contain(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        self.fill_payloads(tick, ids)?;
        self.replan_entering_walks(tick, ids, budgets)?;
        self.absorb_arrivals(tick, ids)?;
        self.toss_grabbed(tick, ids)?;
        self.hold_passengers(tick, ids)?;
        self.release_passengers(tick, ids)
    }

    fn replan_entering_walks(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let walking: Vec<(Entity, Entity)> = tick
            .state()
            .store(ids.entering)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, entering)| !entering.through)
                    .map(|(mover, entering)| (mover, entering.container))
                    .collect()
            })
            .unwrap_or_default();
        for (mover, container) in walking {
            if !tick.state().is_live(mover) || !tick.state().is_live(container) {
                continue;
            }
            let Some(entry) = self.contain_entry_point(tick.state(), ids, container) else {
                continue;
            };
            let Some(position) = tick
                .state()
                .get(ids.body, mover)
                .map(|body| body.0.position)
            else {
                continue;
            };
            self.close_on_target_boarding(tick, ids, mover, position, entry, budgets)?;
        }
        Ok(())
    }

    fn toss_grabbed(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        if self.grab_tosses.iter().all(Option::is_none) {
            return Ok(());
        }
        let now = tick.frame();
        let grabbed: Vec<(Entity, Entity, Frame)> = tick
            .state()
            .store(ids.contained)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(entity, held)| {
                        held.grabbed_at.map(|at| (entity, held.container, at))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (victim, holder, since) in grabbed {
            let world = tick.state();
            let Some(toss) = world
                .get(ids.kind, holder)
                .and_then(|kind| self.grab_tosses.get(usize::from(kind.0)))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let held_for = u16::try_from(now.get().saturating_sub(since.get())).unwrap_or(u16::MAX);
            let timed = toss
                .time
                .is_some_and(|seconds| f32::from(held_for) >= seconds * Self::ticks_per_second());
            let high = toss.height.is_some_and(|height| {
                world.get(ids.body, holder).is_some_and(|body| {
                    let at = body.0.position;
                    Self::feet(world, ids, holder) - self.ground_height(at.x, at.y) >= height
                })
            });
            if !(timed || high) {
                continue;
            }
            diag!(
                "contain: frame {} entity {} tosses entity {} after {held_for} tick(s){}",
                now.get(),
                holder.index(),
                victim.index(),
                if high { ", high enough" } else { "" }
            );
            let fx = toss.fx.clone();
            let at = Self::fx_point_of(world, ids, victim);
            if let (Some(list), Some(at)) = (fx.as_deref(), at) {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    list,
                    at,
                    crate::fx::FxSource::Weapon,
                    victim,
                    Some(holder),
                );
            }
            let carried = tick
                .state()
                .get(ids.body, holder)
                .map_or(0.0, |body| body.0.speed.max(0.0));
            self.drop_passenger(tick, ids, holder, victim, [carried, 0.0, 0.0], None, true)?;
        }
        Ok(())
    }

    fn absorb_arrivals(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let entering: Vec<(Entity, Entering)> = tick
            .state()
            .store(ids.entering)
            .map(|store| store.iter().map(|(entity, held)| (entity, *held)).collect())
            .unwrap_or_default();
        for (mover, held) in entering {
            let container = held.container;
            let world = tick.state();
            if !world.is_live(mover) || !world.is_live(container) {
                tick.state_mut().remove(ids.entering, mover);
                continue;
            }
            let Some(way) = self.doorway(world, ids, container) else {
                tick.state_mut().remove(ids.entering, mover);
                continue;
            };
            if let Some(horde) = world.get(ids.member, mover).map(|member| member.horde) {
                let admitted = world
                    .get(ids.contained, horde)
                    .is_some_and(|held| held.container == container);
                if !admitted {
                    Self::stop_entering(tick, ids, mover, held);
                    continue;
                }
                let walked_in = self.walk_in(tick, ids, &way, mover, held)?;
                let pulled = !walked_in && self.straggler_forces(tick, ids, horde);
                if walked_in || pulled {
                    tick.state_mut().remove(ids.entering, mover);
                    self.take_in(tick, ids, container, &[mover])?;
                    self.mark_member_entered(tick, ids, horde)?;
                    diag!(
                        "contain: member {} of entity {} went into entity {} on frame {}{}",
                        mover.index(),
                        horde.index(),
                        container.index(),
                        tick.frame().get(),
                        if pulled { " (straggler pull)" } else { "" }
                    );
                    if let Some((percent, passenger_filter)) = self
                        .contain_view(tick.state(), container)
                        .map(|view| (view.cash_back, view.open.passenger_filter().clone()))
                        .filter(|(percent, _)| *percent > 0.0)
                    {
                        if self.passes_passenger_filter(tick.state(), ids, &passenger_filter, horde)
                        {
                            if let Some(member_owner) =
                                Self::controller_of(tick.state(), ids, mover)
                            {
                                self.slaughter_pay(
                                    tick,
                                    ids,
                                    container,
                                    member_owner,
                                    mover,
                                    percent,
                                )?;
                            }
                            if self.horde_fully_in(tick.state(), ids, horde, container) {
                                let victims = Self::object_and_members(tick.state(), ids, horde);
                                self.slaughter_kill(tick, ids, &victims)?;
                            }
                        }
                    }
                } else if !held.through && Self::approach_failed(tick, ids, &way, mover, held) {
                    Self::stop_entering(tick, ids, mover, held);
                }
                continue;
            }
            let Some(owner) = Self::controller_of(world, ids, mover) else {
                Self::stop_entering(tick, ids, mover, held);
                continue;
            };
            if self
                .contain_admits(world, ids, owner, container, mover)
                .is_err()
            {
                Self::stop_entering(tick, ids, mover, held);
                continue;
            }
            let arrived = if world.contains(ids.horde, mover) {
                Self::at_door(world, ids, &way, mover)
            } else {
                self.walk_in(tick, ids, &way, mover, held)?
            };
            if arrived {
                tick.state_mut().remove(ids.entering, mover);
                self.absorb(tick, ids, container, mover, owner)?;
            } else if !held.through && Self::approach_failed(tick, ids, &way, mover, held) {
                Self::stop_entering(tick, ids, mover, held);
            }
        }
        Ok(())
    }

    fn approach_failed(
        tick: &Tick<'_>,
        ids: Ids,
        way: &Doorway,
        mover: Entity,
        held: Entering,
    ) -> bool {
        let world = tick.state();
        let elapsed = tick
            .frame()
            .since(held.since)
            .map_or(0, mint_sim::LogicFrames::get);
        elapsed >= WAIT_TO_FORCE_MEMBER_TO_ENTER_DELAY
            && !world.contains(ids.path, mover)
            && !Self::within(world, ids, mover, way.door, ENTRY_REACH)
            && !Self::against(world, ids, way, mover, ENTRY_REACH)
    }

    fn doorway(&self, world: &World, ids: Ids, container: Entity) -> Option<Doorway> {
        let view = self.contain_view(world, container)?;
        let body = world.get(ids.body, container)?;
        let (centre, facing) = (body.0.position, body.0.facing);
        Some(Doorway {
            centre,
            radius: Self::radius_of(world, ids, container),
            door: in_frame(centre, facing, view.open.entry_offset()),
            inside: in_frame(centre, facing, view.open.entry_position()),
        })
    }

    fn radius_of(world: &World, ids: Ids, entity: Entity) -> f32 {
        world
            .get(ids.shape, entity)
            .map_or(0.0, |shape: &crate::unit::Shape| shape.radius)
    }

    fn within(world: &World, ids: Ids, object: Entity, point: Vec2, reach: f32) -> bool {
        world
            .get(ids.body, object)
            .is_some_and(|body| body.0.position.minus(point).length() <= reach)
    }

    fn against(world: &World, ids: Ids, way: &Doorway, object: Entity, slack: f32) -> bool {
        let reach = way.radius + Self::radius_of(world, ids, object) + slack;
        Self::within(world, ids, object, way.centre, reach)
    }

    fn at_door(world: &World, ids: Ids, way: &Doorway, horde: Entity) -> bool {
        let stopped = !world.contains(ids.path, horde);
        Self::object_and_members(world, ids, horde)
            .iter()
            .any(|entity| {
                Self::within(world, ids, *entity, way.door, ENTRY_REACH)
                    || (stopped && Self::against(world, ids, way, *entity, ENTRY_REACH))
            })
    }

    fn walk_in(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        way: &Doorway,
        mover: Entity,
        held: Entering,
    ) -> Result<bool> {
        let world = tick.state();
        let stopped = !world.contains(ids.path, mover);
        let parked = stopped && Self::against(world, ids, way, mover, ENTRY_REACH);
        if held.through {
            let arrived = Self::within(world, ids, mover, way.inside, ENTRY_REACH)
                || Self::against(world, ids, way, mover, 0.0)
                || parked;
            if arrived {
                return Ok(true);
            }
            let elapsed = tick
                .frame()
                .since(held.since)
                .map_or(0, mint_sim::LogicFrames::get);
            return Ok(stopped && elapsed >= WAIT_TO_FORCE_MEMBER_TO_ENTER_DELAY);
        }
        if Self::within(world, ids, mover, way.door, ENTRY_REACH) || parked {
            tick.state_mut().insert(
                ids.entering,
                mover,
                Entering {
                    through: true,
                    ..held
                },
            )?;
            Self::route(tick, ids, mover, way.inside)?;
            tick.state_mut().insert(
                ids.ignored_obstacle,
                mover,
                IgnoredObstacle(held.container),
            )?;
        }
        Ok(false)
    }

    fn straggler_forces(&self, tick: &Tick<'_>, ids: Ids, horde: Entity) -> bool {
        tick.state()
            .get(ids.straggler_clock, horde)
            .is_some_and(|clock| {
                tick.frame()
                    .since(clock.0)
                    .map_or(0, mint_sim::LogicFrames::get)
                    >= WAIT_TO_FORCE_MEMBER_TO_ENTER_DELAY
            })
    }

    fn mark_member_entered(&self, tick: &mut Tick<'_>, ids: Ids, horde: Entity) -> Result<()> {
        let now = tick.frame();
        tick.state_mut()
            .insert(ids.straggler_clock, horde, StragglerClock(now))?;
        Ok(())
    }

    fn route(tick: &mut Tick<'_>, ids: Ids, mover: Entity, goal: Vec2) -> Result<()> {
        if let Some(mut body) = tick.state().get(ids.body, mover).copied() {
            body.0.waypoint = 0;
            tick.state_mut().insert(ids.body, mover, body)?;
        }
        tick.state_mut().insert(
            ids.path,
            mover,
            crate::unit::Path {
                route: Route::to_point(goal),
                goal,
                kind: crate::unit::PathKind::Ordered,
                facing: None,
            },
        )?;
        Ok(())
    }

    fn stop_entering(tick: &mut Tick<'_>, ids: Ids, mover: Entity, held: Entering) {
        tick.state_mut().remove(ids.entering, mover);
        if held.through || tick.state().contains(ids.member, mover) {
            tick.state_mut().remove(ids.path, mover);
            tick.state_mut().remove(ids.ignored_obstacle, mover);
        }
    }

    pub(super) fn absorb(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
        owner: PlayerId,
    ) -> Result<()> {
        let Some((cash_back, ring, passenger_filter)) =
            self.contain_view(tick.state(), container).map(|v| {
                (
                    v.cash_back,
                    v.ring.cloned(),
                    v.open.passenger_filter().clone(),
                )
            })
        else {
            return Ok(());
        };
        let delivering = ring
            .as_ref()
            .is_some_and(|ring| self.carries_status(tick.state(), object, ring.status()));
        if let Some(ring) = &ring {
            self.ring_entry(tick, ids, container, object, owner, ring)?;
        }
        if cash_back > 0.0
            && !tick.state().contains(ids.horde, object)
            && self.passes_passenger_filter(tick.state(), ids, &passenger_filter, object)
        {
            return self.slaughter(tick, ids, container, object, owner, cash_back);
        }
        let mut list = tick
            .state()
            .get(ids.passengers, container)
            .cloned()
            .unwrap_or(Passengers(Vec::new(), None));
        let before = list.0.clone();
        list.0.push(object);
        let types = self.passenger_type_effects(tick.state(), ids, container, &before, &list.0);
        tick.state_mut().insert(ids.passengers, container, list)?;
        let types: Vec<(Entity, Effect)> = types
            .into_iter()
            .map(|effect| (container, effect))
            .collect();
        self.apply_effects(tick, ids, types)?;
        self.garrison_follows_passengers(tick, ids, container)?;
        let Some(door) = self.contain_entry_point(tick.state(), ids, container) else {
            return Ok(());
        };
        self.take_in(tick, ids, container, &[object])?;
        if delivering {
            if let Some(mut held) = tick.state().get(ids.contained, object).copied() {
                held.leaves_at = tick.frame();
                tick.state_mut().insert(ids.contained, object, held)?;
            }
        }
        let members = Self::object_and_members(tick.state(), ids, object);
        let walking = members.get(1..).unwrap_or_default();
        let since = tick.frame();
        for member in walking {
            tick.state_mut().remove(ids.selected, *member);
            tick.state_mut().remove(ids.attack_order, *member);
            tick.state_mut().remove(ids.ground_order, *member);
            tick.state_mut().insert(
                ids.entering,
                *member,
                Entering {
                    container,
                    through: false,
                    since,
                },
            )?;
            Self::route(tick, ids, *member, door)?;
        }
        diag!(
            "contain: entity {} took entity {} - {} passenger(s) inside, {} walking in through the door",
            container.index(),
            object.index(),
            tick.state()
                .get(ids.passengers, container)
                .map_or(0, |list| list.0.len()),
            walking.len()
        );
        Ok(())
    }

    pub(super) fn take_in(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        entities: &[Entity],
    ) -> Result<()> {
        let status_names = self
            .contain_view(tick.state(), container)
            .map(|view| view.open.status_of_contained().to_vec())
            .unwrap_or_default();
        let held = Contained {
            container,
            status: ContainStatus::from_names(&status_names),
            leaves_at: Frame::FOREVER,
            grabbed_at: None,
        };
        for &entity in entities {
            self.leave_flight(tick, ids, entity)?;
            tick.state_mut().insert(ids.contained, entity, held)?;
            tick.state_mut().remove(ids.entering, entity);
            tick.state_mut().remove(ids.selected, entity);
            tick.state_mut().remove(ids.path, entity);
            tick.state_mut().remove(ids.ignored_obstacle, entity);
            tick.state_mut().remove(ids.attack_order, entity);
            tick.state_mut().remove(ids.ground_order, entity);
        }
        if !status_names.is_empty() {
            let effect = Effect::StatusBits {
                set: status_names.into(),
                clear: names(Vec::new()),
            };
            let entries = entities
                .iter()
                .map(|entity| (*entity, effect.clone()))
                .collect();
            self.apply_effects(tick, ids, entries)?;
        }
        Ok(())
    }

    pub(super) fn object_and_members(world: &World, ids: Ids, object: Entity) -> Vec<Entity> {
        let mut out = vec![object];
        if let Some(horde) = world.get(ids.horde, object) {
            out.extend(horde.members.iter().copied());
        }
        out
    }

    fn slaughter(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
        owner: PlayerId,
        percent: f32,
    ) -> Result<()> {
        self.slaughter_pay(tick, ids, container, owner, object, percent)?;
        self.slaughter_kill(tick, ids, &[object])
    }

    fn slaughter_pay(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        owner: PlayerId,
        victim: Entity,
        percent: f32,
    ) -> Result<()> {
        let Some(record) = tick
            .state()
            .get(ids.kind, victim)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return Ok(());
        };
        let modifiers = self.cost_modifiers_for_record(tick.state(), ids, owner, record);
        let amount = crate::construction::bounty_refund(record, percent, &modifiers);
        let amount = crate::economy::deposited(amount, self.multiplay.money(self.players), 1.0);
        if amount <= 0 {
            return Ok(());
        }
        if let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        {
            let mut purse = tick
                .state()
                .get(ids.purse, player_entity)
                .map_or(mint_sage_economy::Money::new(0), |held| held.0);
            purse.deposit(amount);
            tick.state_mut()
                .insert(ids.purse, player_entity, crate::economy::Purse(purse))?;
        }
        let scalar = self.level_scalar(tick.state(), ids, container);
        let gain = crate::experience::scaled(u32::try_from(amount).unwrap_or(u32::MAX), scalar);
        if gain > 0 {
            self.add_experience(tick, ids, container, gain)?;
        }
        diag!(
            "contain: entity {} slaughtered entity {} for {amount} at {:.0}%",
            container.index(),
            victim.index(),
            percent * 100.0
        );
        Ok(())
    }

    fn slaughter_kill(&self, tick: &mut Tick<'_>, ids: Ids, victims: &[Entity]) -> Result<()> {
        let mut killed: Vec<(Entity, f32)> = Vec::new();
        for victim in victims {
            if let Some(mut health) = tick.state().get(ids.health, *victim).copied() {
                health.0.set_current(0.0);
                tick.state_mut().insert(ids.health, *victim, health)?;
                killed.push((*victim, 0.0));
            }
        }
        self.resolve_deaths(tick, ids, &killed)
    }

    fn horde_fully_in(&self, world: &World, ids: Ids, horde: Entity, container: Entity) -> bool {
        world.get(ids.horde, horde).is_some_and(|horde| {
            horde.members.iter().all(|member| {
                world
                    .get(ids.contained, *member)
                    .is_some_and(|held| held.container == container)
            })
        })
    }

    fn ring_entry(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
        owner: PlayerId,
        ring: &RingEntry,
    ) -> Result<()> {
        if !self.carries_status(tick.state(), object, ring.status()) {
            return Ok(());
        }
        for name in ring.upgrades() {
            match self.upgrade_bits.bit(name) {
                Some(bit) if self.upgrade_bits.is_player(bit) => {
                    self.grant_player_upgrade(tick, ids, owner, bit)?;
                }
                Some(bit) => self.grant_object_upgrade(tick, ids, container, bit)?,
                None => diag!("contain: {name} is not an Upgrade - nothing granted"),
            }
        }
        let destroyed = self.closest_to_destroy(tick.state(), ids, container, ring);
        if let Some(entity) = destroyed {
            tick.state_mut().despawn(entity);
        }
        self.apply_effects(
            tick,
            ids,
            vec![(
                object,
                Effect::StatusBits {
                    set: names(Vec::new()),
                    clear: names(vec![ring.status().to_owned()]),
                },
            )],
        )?;
        if let Some(list) = ring.fx() {
            if let Some(at) = Self::fx_point_of(tick.state(), ids, container) {
                self.emit_fx_object_named(
                    tick,
                    ids,
                    list,
                    at,
                    crate::fx::FxSource::Power,
                    container,
                    None,
                );
            }
        }
        diag!(
            "contain: Ring entry at entity {} - {} upgrade(s), {} object(s) destroyed",
            container.index(),
            ring.upgrades().len(),
            usize::from(destroyed.is_some())
        );
        Ok(())
    }

    fn closest_to_destroy(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        ring: &RingEntry,
    ) -> Option<Entity> {
        const RANGE: f32 = 9999.0;
        let at = world.get(ids.body, container)?.0.position;
        let store = world.store(ids.body)?;
        store
            .iter()
            .filter(|(entity, _)| *entity != container)
            .filter_map(|(entity, body)| {
                let distance = body.0.position.minus(at).length();
                (distance <= RANGE
                    && self.passes_passenger_filter(world, ids, ring.to_destroy(), entity))
                .then_some((entity, distance))
            })
            .min_by(|(a_entity, a_distance), (b_entity, b_distance)| {
                a_distance
                    .partial_cmp(b_distance)
                    .unwrap_or(core::cmp::Ordering::Equal)
                    .then_with(|| a_entity.index().cmp(&b_entity.index()))
            })
            .map(|(entity, _)| entity)
    }

    fn carries_status(&self, world: &World, entity: Entity, status: &str) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(world, entity, status))
    }

    fn hold_passengers(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let held: Vec<(Entity, Entity)> = tick
            .state()
            .store(ids.contained)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, held)| (entity, held.container))
                    .collect()
            })
            .unwrap_or_default();
        let mut containers: Vec<Entity> = held.iter().map(|(_, container)| *container).collect();
        containers.sort_unstable_by_key(|entity| entity.index());
        containers.dedup();
        let mut perched: BTreeMap<Entity, crate::perches::Perch> = BTreeMap::new();
        for container in containers {
            self.perch_passengers(tick.state(), ids, container, &mut perched);
        }
        for (entity, container) in held {
            let world = tick.state();
            if !world.is_live(container) {
                tick.state_mut().remove(ids.contained, entity);
                continue;
            }
            let (Some(view), Some(anchor)) = (
                self.contain_view(world, container),
                world.get(ids.body, container).copied(),
            ) else {
                continue;
            };
            let feet = Self::feet(world, ids, container);
            let (point, height, facing) = match perched.get(&entity) {
                Some(perch) => (
                    in_frame(
                        anchor.0.position,
                        anchor.0.facing,
                        Offset {
                            x: perch.position[0],
                            y: perch.position[1],
                        },
                    ),
                    feet + perch.position[2],
                    Some(Facing::from_radians(
                        anchor.0.facing.radians() + perch.facing.radians(),
                    )),
                ),
                None => (
                    in_frame(
                        anchor.0.position,
                        anchor.0.facing,
                        view.open.entry_position(),
                    ),
                    feet,
                    None,
                ),
            };
            if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                body.0.position = point;
                if let Some(facing) = facing {
                    body.0.facing = facing;
                }
                body.0.speed = 0.0;
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, entity, body)?;
            }
            tick.state_mut()
                .insert(ids.elevation, entity, crate::unit::Elevation::new(height))?;
            tick.state_mut().remove(ids.path, entity);
        }
        Ok(())
    }

    fn perch_passengers(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        out: &mut BTreeMap<Entity, crate::perches::Perch>,
    ) {
        let Some(bones) = self.perches_of(world, ids, container) else {
            return;
        };
        for (entity, slot) in self.perch_slots(world, ids, container, bones.len()) {
            if let Some(bone) = bones.get(slot) {
                out.insert(entity, *bone);
            }
        }
    }

    fn perches_of(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
    ) -> Option<&[crate::perches::Perch]> {
        world
            .get(ids.kind, container)
            .and_then(|kind| self.passenger_perches.get(usize::from(kind.0)))
            .map(Vec::as_slice)
            .filter(|bones| !bones.is_empty())
    }

    fn perch_slots(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        bones: usize,
    ) -> Vec<(Entity, usize)> {
        let (Some(view), Some(list)) = (
            self.contain_view(world, container),
            world.get(ids.passengers, container),
        ) else {
            return Vec::new();
        };
        let wanted = view.open.passenger_bone_kind();
        let mut slots = Vec::new();
        let mut next = 0usize;
        for object in &list.0 {
            for entity in Self::object_and_members(world, ids, *object) {
                let inside = world
                    .get(ids.contained, entity)
                    .is_some_and(|held| held.container == container);
                let admitted = wanted.is_none_or(|bit| self.has_kind_bit(world, ids, entity, bit));
                if !inside || !admitted {
                    continue;
                }
                if let Some(slot) = next.checked_rem(bones) {
                    slots.push((entity, slot));
                }
                next = next.saturating_add(1);
            }
        }
        slots
    }

    pub(crate) fn riders(&self, world: &World, ids: Ids) -> Vec<Rider> {
        let mut containers: Vec<Entity> = world
            .store(ids.contained)
            .map(|store| store.iter().map(|(_, held)| held.container).collect())
            .unwrap_or_default();
        containers.sort_unstable_by_key(|entity| entity.index());
        containers.dedup();
        let mut riders = Vec::new();
        for container in containers {
            let (Some(bones), Some(prefix)) = (
                self.perches_of(world, ids, container),
                self.contain_view(world, container)
                    .and_then(|view| view.open.passenger_bone_prefix().map(str::to_owned)),
            ) else {
                continue;
            };
            riders.extend(
                self.perch_slots(world, ids, container, bones.len())
                    .into_iter()
                    .map(|(rider, slot)| Rider {
                        rider,
                        container,
                        slot,
                        prefix: prefix.clone(),
                    }),
            );
        }
        riders.sort_unstable_by_key(|rider| rider.rider.index());
        riders
    }

    fn release_passengers(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<(Entity, Entity)> = tick
            .state()
            .store(ids.contained)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.leaves_at <= now)
                    .map(|(entity, held)| (entity, held.container))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, container) in due {
            self.release_one(tick, ids, container, entity)?;
        }
        Ok(())
    }

    fn siege_ladder_bone(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
    ) -> Option<crate::perches::Perch> {
        let kind = world.get(ids.kind, container)?;
        *self.siege_ladder_exit.get(usize::from(kind.0))?
    }

    fn deployed_siege_exit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
    ) -> Result<bool> {
        if !Self::siege_deployed(tick.state(), ids, container) {
            return Ok(false);
        }
        let Some(bone) = self.siege_ladder_bone(tick.state(), ids, container) else {
            diag!(
                "contain: entity {} is siege-deployed with no Ladder04 bone, ordinary exit",
                container.index()
            );
            return Ok(false);
        };
        let world = tick.state();
        let Some(anchor) = world.get(ids.body, container).copied() else {
            return Ok(false);
        };
        let feet = Self::feet(world, ids, container);
        let point = in_frame(
            anchor.0.position,
            anchor.0.facing,
            Offset {
                x: bone.position[0],
                y: bone.position[1],
            },
        );
        let height = feet + bone.position[2];
        let facing = Facing::from_radians(anchor.0.facing.radians() + bone.facing.radians());
        let members = self.take_out(tick, ids, container, object)?;
        for entity in &members {
            if let Some(mut body) = tick.state().get(ids.body, *entity).copied() {
                body.0.position = point;
                body.0.facing = facing;
                body.0.speed = 0.0;
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, *entity, body)?;
            }
            tick.state_mut()
                .insert(ids.elevation, *entity, crate::unit::Elevation::new(height))?;
            tick.state_mut().insert(
                ids.layer,
                *entity,
                crate::wall_layer::Layer(crate::wall_layer::FIRST_WALL_TAG),
            )?;
            tick.state_mut().remove(ids.path, *entity);
            tick.state_mut().remove(ids.ignored_obstacle, *entity);
        }
        diag!(
            "contain: entity {} left entity {} at the Ladder04 bone, layer 17",
            object.index(),
            container.index()
        );
        Ok(true)
    }

    fn release_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
    ) -> Result<()> {
        if self.deployed_siege_exit(tick, ids, container, object)? {
            return Ok(());
        }
        let world = tick.state();
        let door = self.contain_view(world, container).and_then(|view| {
            let body = world.get(ids.body, container)?;
            Some(in_frame(
                body.0.position,
                body.0.facing,
                view.open.exit_offset(),
            ))
        });
        let is_transport = self
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.transport_view(world, container).is_some());
        let exit = self.contain_view(world, container).and_then(|view| {
            let body = world.get(ids.body, container)?;
            let base = view.open.exit_offset();
            let point = if is_transport {
                let paths = view.open.number_of_exit_paths().max(1);
                let index = world
                    .get(ids.passengers, container)
                    .and_then(|list| list.0.iter().position(|held| *held == object))
                    .unwrap_or(0);
                #[allow(clippy::cast_precision_loss)]
                let turn = (index % paths as usize) as f32 / paths as f32 * core::f32::consts::TAU;
                let (sin, cos) = mint_math::f32::sin_cos(turn);
                Offset {
                    x: base.x * cos - base.y * sin,
                    y: base.x * sin + base.y * cos,
                }
            } else {
                base
            };
            Some(in_frame(body.0.position, body.0.facing, point))
        });
        let members = self.take_out(tick, ids, container, object)?;
        if let (Some(door), Some(point)) = (door, exit) {
            self.walk_out(tick, ids, container, object, &members, door, point)?;
        }
        diag!(
            "contain: entity {} left entity {} at frame {}",
            object.index(),
            container.index(),
            tick.frame().get()
        );
        Ok(())
    }

    pub(super) fn fling_passengers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        holder: Entity,
        power: &str,
        velocity: [f32; 3],
        landing_warhead: Option<&str>,
    ) -> Result<()> {
        let warhead = landing_warhead.and_then(|name| {
            let id = self.combat.weapons.id(name);
            if id.is_none() {
                diag!("contain: FlingPassengerLandingWarhead {name} is not declared");
            }
            id
        });
        let held = tick
            .state()
            .get(ids.passengers, holder)
            .map(|list| list.0.clone())
            .unwrap_or_default();
        diag!(
            "contain: frame {} entity {} lets go of {} passenger(s) with {power}",
            tick.frame().get(),
            holder.index(),
            held.len()
        );
        for passenger in held {
            match warhead {
                Some(warhead) if !tick.state().contains(ids.horde, passenger) => {
                    self.drop_passenger(
                        tick,
                        ids,
                        holder,
                        passenger,
                        velocity,
                        Some(warhead),
                        false,
                    )?;
                }
                _ => self.release_one(tick, ids, holder, passenger)?,
            }
        }
        Ok(())
    }

    pub(super) fn passenger_admitted(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        filter: &ObjectFilter,
    ) -> Option<Entity> {
        world
            .get(ids.passengers, container)?
            .0
            .iter()
            .copied()
            .filter(|held| !world.contains(ids.horde, *held))
            .find(|held| {
                world
                    .get(ids.kind, *held)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| filter.passes(&entry.record.kind_of, &entry.record.name))
            })
    }

    pub(super) fn take_out(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
    ) -> Result<Vec<Entity>> {
        if let Some(mut list) = tick.state().get(ids.passengers, container).cloned() {
            let before = list.0.clone();
            list.0.retain(|held| *held != object);
            let types = self.passenger_type_effects(tick.state(), ids, container, &before, &list.0);
            let types: Vec<(Entity, Effect)> = types
                .into_iter()
                .map(|effect| (container, effect))
                .collect();
            self.apply_effects(tick, ids, types)?;
            tick.state_mut().insert(ids.passengers, container, list)?;
            self.garrison_follows_passengers(tick, ids, container)?;
        }
        let status_names = self
            .contain_view(tick.state(), container)
            .map(|view| view.open.status_of_contained().to_vec())
            .unwrap_or_default();
        let members = Self::object_and_members(tick.state(), ids, object);
        for entity in &members {
            tick.state_mut().remove(ids.contained, *entity);
            if tick.state_mut().remove(ids.entering, *entity).is_some() {
                tick.state_mut().remove(ids.path, *entity);
            }
        }
        let grabbed = |entity: &Entity| {
            self.modules.as_ref().is_some_and(|wiring| {
                tick.state()
                    .get(wiring.state, *entity)
                    .is_some_and(|state| {
                        state
                            .model_conditions
                            .iter()
                            .any(|held| held.eq_ignore_ascii_case(GRABBED))
                    })
            })
        };
        let mut released: Vec<(Entity, Effect)> = members
            .iter()
            .filter(|entity| grabbed(entity))
            .map(|entity| {
                (
                    *entity,
                    Effect::ModelConditions {
                        add: names(Vec::new()),
                        remove: names(vec![GRABBED.to_owned(), HELD_ALOFT.to_owned()]),
                    },
                )
            })
            .collect();
        if !status_names.is_empty() {
            let effect = Effect::StatusBits {
                set: names(Vec::new()),
                clear: status_names.into(),
            };
            released.extend(members.iter().map(|entity| (*entity, effect.clone())));
        }
        self.apply_effects(tick, ids, released)?;
        Ok(members)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_out(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        object: Entity,
        members: &[Entity],
        door: Vec2,
        point: Vec2,
    ) -> Result<()> {
        let mover = self.mover_class(tick, ids, object);
        let placed = self
            .passable_cell_for(tick, ids, point, mover)
            .map_or(point, |cell| self.passability.geometry.centre_of(cell));
        let facing = tick
            .state()
            .get(ids.body, container)
            .map(|body| body.0.facing);
        if Self::is_immobile(tick, ids, object) {
            if let Some(mut body) = tick.state().get(ids.body, object).copied() {
                body.0.position = placed;
                if let Some(facing) = facing {
                    body.0.facing = facing;
                }
                body.0.speed = 0.0;
                body.0.waypoint = 0;
                tick.state_mut().insert(ids.body, object, body)?;
            }
        } else {
            if let Some(inside) = self
                .doorway(tick.state(), ids, container)
                .map(|way| way.inside)
            {
                for entity in members {
                    if let Some(mut body) = tick.state().get(ids.body, *entity).copied() {
                        body.0.position = inside;
                        if let Some(facing) = facing {
                            body.0.facing = facing;
                        }
                        tick.state_mut().insert(ids.body, *entity, body)?;
                    }
                }
            }
            tick.state_mut().insert(
                ids.path,
                object,
                crate::unit::Path {
                    route: Route {
                        points: vec![door, placed],
                        extra_distance: 0.0,
                        portals: Vec::new(),
                    },
                    goal: placed,
                    kind: crate::unit::PathKind::Ordered,
                    facing: None,
                },
            )?;
            tick.state_mut()
                .insert(ids.ignored_obstacle, object, IgnoredObstacle(container))?;
            for member in members.get(1..).unwrap_or_default() {
                Self::route(tick, ids, *member, door)?;
                tick.state_mut().insert(
                    ids.ignored_obstacle,
                    *member,
                    IgnoredObstacle(container),
                )?;
            }
        }
        Ok(())
    }

    fn contain_count(&self, world: &World, ids: Ids, container: Entity) -> u32 {
        let held = |entity: Entity| {
            world
                .get(ids.passengers, entity)
                .map_or(0, |list| list.0.len())
        };
        let counted = if self.is_tunnel(world, container) {
            self.tunnel_pool(world, ids, container)
                .into_iter()
                .map(held)
                .sum()
        } else {
            held(container)
        };
        u32::try_from(counted).unwrap_or(u32::MAX)
    }

    fn is_tunnel(&self, world: &World, entity: Entity) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|wiring| wiring.is_tunnel(world, entity))
    }

    fn tunnel_pool(&self, world: &World, ids: Ids, of: Entity) -> Vec<Entity> {
        let Some(wiring) = self.modules.as_ref() else {
            return Vec::new();
        };
        let owner = world.get(ids.owner, of).map(|held| held.0);
        wiring
            .tunnels(world)
            .into_iter()
            .filter(|tunnel| world.get(ids.owner, *tunnel).map(|held| held.0) == owner)
            .collect()
    }

    fn gather_tunnel_pool(&self, tick: &mut Tick<'_>, ids: Ids, to: Entity) -> Result<()> {
        if !self.is_tunnel(tick.state(), to) {
            return Ok(());
        }
        let others: Vec<Entity> = self
            .tunnel_pool(tick.state(), ids, to)
            .into_iter()
            .filter(|tunnel| *tunnel != to)
            .collect();
        let mut moved = 0usize;
        for tunnel in others {
            moved = moved.saturating_add(self.move_passengers(tick, ids, tunnel, to)?);
        }
        if moved > 0 {
            diag!(
                "contain: {moved} tunnel passenger(s) pooled into entity {}",
                to.index()
            );
        }
        Ok(())
    }

    fn move_passengers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        from: Entity,
        to: Entity,
    ) -> Result<usize> {
        let Some(list) = tick.state().get(ids.passengers, from).cloned() else {
            return Ok(0);
        };
        tick.state_mut().remove(ids.passengers, from);
        let mut held_by = tick
            .state()
            .get(ids.passengers, to)
            .cloned()
            .unwrap_or(Passengers(Vec::new(), None));
        for passenger in &list.0 {
            for entity in Self::object_and_members(tick.state(), ids, *passenger) {
                if let Some(mut held) = tick.state().get(ids.contained, entity).copied() {
                    held.container = to;
                    tick.state_mut().insert(ids.contained, entity, held)?;
                }
            }
            held_by.0.push(*passenger);
        }
        let moved = list.0.len();
        tick.state_mut().insert(ids.passengers, to, held_by)?;
        Ok(moved)
    }

    pub(super) fn grab_on_hit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        victim: Entity,
        remove_from_other: bool,
    ) -> Result<bool> {
        if grabber == victim || !tick.state().is_live(victim) {
            return Ok(false);
        }
        let held_by = tick
            .state()
            .get(ids.contained, victim)
            .map(|held| held.container);
        if held_by == Some(grabber) || (held_by.is_some() && !remove_from_other) {
            return Ok(false);
        }
        let grabs_structures = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.transport_view(tick.state(), grabber))
            .and_then(|module| module.grab())
            .is_some_and(|grab| grab.structures);
        let is_structure = tick
            .state()
            .get(ids.kind, victim)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
        if is_structure && !grabs_structures {
            return Ok(false);
        }
        let Some(owner) = tick.state().get(ids.owner, victim).map(|held| held.0) else {
            return Ok(false);
        };
        if self
            .contain_admits(tick.state(), ids, owner, grabber, victim)
            .is_err()
        {
            return Ok(false);
        }
        self.take_into(tick, ids, grabber, victim)?;
        Ok(true)
    }

    pub(super) fn take_into(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        grabber: Entity,
        victim: Entity,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, victim).map(|held| held.0) else {
            return Ok(());
        };
        let grab = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.transport_view(tick.state(), grabber))
            .and_then(|module| module.grab())
            .map(|grab| (grab.weapon.clone(), grab.on_victim));
        if let Some(container) = tick
            .state()
            .get(ids.contained, victim)
            .map(|held| held.container)
        {
            self.release_one(tick, ids, container, victim)?;
        }
        self.leave_horde(tick, ids, victim)?;
        self.absorb(tick, ids, grabber, victim, owner)?;
        let now = tick.frame();
        if let Some(held) = tick.state_mut().get_mut(ids.contained, victim) {
            held.grabbed_at = Some(now);
        }
        let mut effects = vec![(
            victim,
            Effect::ModelConditions {
                add: names(vec![GRABBED.to_owned(), HELD_ALOFT.to_owned()]),
                remove: names(Vec::new()),
            },
        )];
        diag!(
            "contain: entity {} grabbed entity {}{}",
            grabber.index(),
            victim.index(),
            grab.as_ref()
                .map_or_else(String::new, |(weapon, ..)| format!(" and fires {weapon}"))
        );
        if let Some((weapon, on_victim)) = grab {
            let at = if on_victim {
                mint_sage_modules::effects::PowerPlace::Object(victim)
            } else {
                mint_sage_modules::effects::PowerPlace::Source
            };
            effects.push((
                grabber,
                Effect::FireWeapon {
                    weapon: Name::from(weapon),
                    at,
                    offset: (0.0, 0.0),
                    repeat: None,
                    alive_only: false,
                    reload_slot: None,
                },
            ));
        }
        self.apply_effects(tick, ids, effects)
    }

    fn leave_horde(&self, tick: &mut Tick<'_>, ids: Ids, member: Entity) -> Result<()> {
        let Some(horde_entity) = tick.state_mut().remove(ids.member, member).map(|m| m.horde)
        else {
            return Ok(());
        };
        if tick
            .state()
            .get(ids.kind, member)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry.record.has_kind("MACHINE")
                    || entry.record.has_kind("HERO")
                    || entry.record.has_kind("SIEGE_TOWER")
            })
        {
            tick.state_mut().remove(ids.horde_member_mark, member);
        }
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        horde.members.retain(|held| *held != member);
        if horde.members.is_empty() {
            tick.state_mut().remove(ids.path, horde_entity);
            tick.state_mut().despawn(horde_entity);
            crate::horde::drop_orphaned_members(tick.state_mut(), ids.member);
            return Ok(());
        }
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
        Ok(())
    }

    fn fill_payloads(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let mut work: Vec<(Entity, Option<(String, u32)>, Vec<(String, String, u32)>)> = wiring
            .transports(tick.state())
            .into_iter()
            .filter_map(|container| {
                let module = wiring.transport_view(tick.state(), container)?;
                let initial = module
                    .initial_payload()
                    .map(|payload| (payload.template.clone(), payload.count));
                let triggers = module
                    .upgrade_creations()
                    .iter()
                    .map(|entry| (entry.upgrade.clone(), entry.template.clone(), entry.count))
                    .collect::<Vec<_>>();
                (initial.is_some() || !triggers.is_empty())
                    .then_some((container, initial, triggers))
            })
            .collect();
        work.extend(
            wiring
                .sieges(tick.state())
                .into_iter()
                .filter_map(|container| {
                    let module = wiring.siege_contain(tick.state(), container)?;
                    let initial = module
                        .initial_crew()
                        .map(|crew| (crew.template.clone(), crew.count));
                    initial.map(|initial| (container, Some(initial), Vec::new()))
                }),
        );
        for (container, initial, triggers) in work {
            let mut filled = tick
                .state()
                .get(ids.payload_filled, container)
                .copied()
                .unwrap_or_default();
            if let Some((template, count)) = initial {
                if !filled.initial {
                    filled.initial = true;
                    self.fill_one_payload(tick, ids, container, &template, count)?;
                }
            }
            for (ordinal, (upgrade, template, count)) in triggers.into_iter().enumerate() {
                let Ok(ordinal) = u32::try_from(ordinal) else {
                    continue;
                };
                let Some(bit) = 1u32.checked_shl(ordinal) else {
                    continue;
                };
                if filled.triggers & bit != 0 {
                    continue;
                }
                let held = self
                    .upgrade_bits
                    .bit(&upgrade)
                    .is_some_and(|bit| self.effective_upgrades(tick, ids, container).contains(bit));
                if !held {
                    continue;
                }
                filled.triggers |= bit;
                self.fill_one_payload(tick, ids, container, &template, count)?;
            }
            tick.state_mut()
                .insert(ids.payload_filled, container, filled)?;
        }
        Ok(())
    }

    fn fill_one_payload(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
        template: &str,
        count: u32,
    ) -> Result<()> {
        let Some(kind) = self.kinds.kind(template) else {
            diag!("contain: payload {template} is not in this match's templates");
            return Ok(());
        };
        let (Some(body), Some(owner)) = (
            tick.state().get(ids.body, container).copied(),
            tick.state().get(ids.owner, container).map(|held| held.0),
        ) else {
            return Ok(());
        };
        let mut made = 0u32;
        for _ in 0..count {
            if self
                .contain_admits(tick.state(), ids, owner, container, container)
                .is_err()
                && self.contain_count(tick.state(), ids, container)
                    >= self
                        .contain_view(tick.state(), container)
                        .map_or(0, |view| view.capacity)
            {
                break;
            }
            let Some(entity) =
                self.spawn_kind_at(tick, ids, kind, body.0.position, body.0.facing, owner)?
            else {
                break;
            };
            self.absorb(tick, ids, container, entity, owner)?;
            let inside = tick
                .state()
                .get(ids.contained, entity)
                .is_some_and(|held| held.container == container);
            let members = Self::object_and_members(tick.state(), ids, entity);
            if let Some(members) = members.get(1..).filter(|m| inside && !m.is_empty()) {
                self.take_in(tick, ids, container, members)?;
            }
            made = made.saturating_add(1);
        }
        diag!(
            "contain: entity {} took {made} of {template} as payload",
            container.index()
        );
        Ok(())
    }

    fn passenger_types(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        list: &[Entity],
    ) -> ([bool; 2], [bool; 3]) {
        let Some(module) = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.transport_view(world, container))
        else {
            return ([false; 2], [false; 3]);
        };
        let carries = |token: Option<&str>| {
            let Some(token) = token else { return false };
            list.iter().any(|passenger| {
                Self::object_and_members(world, ids, *passenger)
                    .into_iter()
                    .any(|entity| {
                        world
                            .get(ids.kind, entity)
                            .and_then(|kind| self.kinds.get(*kind))
                            .is_some_and(|entry| entry.record.has_kind(token))
                    })
            })
        };
        (
            [
                carries(module.weapon_set_kind(0)),
                carries(module.weapon_set_kind(1)),
            ],
            [
                carries(module.weapon_state_kind(0)),
                carries(module.weapon_state_kind(1)),
                carries(module.weapon_state_kind(2)),
            ],
        )
    }

    fn passenger_type_effects(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
        before: &[Entity],
        after: &[Entity],
    ) -> Vec<Effect> {
        const SET_FLAGS: [&str; 2] = ["PASSENGER_TYPE_ONE", "PASSENGER_TYPE_TWO"];
        const STATES: [&str; 3] = ["WEAPONSTATE_ONE", "WEAPONSTATE_TWO", "WEAPONSTATE_THREE"];
        let (was_set, was_state) = self.passenger_types(world, ids, container, before);
        let (now_set, now_state) = self.passenger_types(world, ids, container, after);
        let mut out = Vec::new();
        for (ordinal, flag) in SET_FLAGS.iter().enumerate() {
            match (was_set.get(ordinal), now_set.get(ordinal)) {
                (Some(false), Some(true)) => {
                    out.push(Effect::WeaponSetFlag(Name::from((*flag).to_owned())));
                }
                (Some(true), Some(false)) => {
                    out.push(Effect::ClearWeaponSetFlag(Name::from((*flag).to_owned())));
                }
                _ => {}
            }
        }
        let (mut add, mut remove) = (Vec::new(), Vec::new());
        for (ordinal, state) in STATES.iter().enumerate() {
            match (was_state.get(ordinal), now_state.get(ordinal)) {
                (Some(false), Some(true)) => add.push((*state).to_owned()),
                (Some(true), Some(false)) => remove.push((*state).to_owned()),
                _ => {}
            }
        }
        if !add.is_empty() || !remove.is_empty() {
            out.push(Effect::ModelConditions {
                add: names(add),
                remove: names(remove),
            });
        }
        out
    }

    pub(super) fn contain_deaths(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[(Entity, f32)],
    ) -> Result<Vec<(Entity, f32)>> {
        for (entity, _) in killed {
            if let Some(held) = tick.state().get(ids.contained, *entity).copied() {
                if let Some(mut list) = tick.state().get(ids.passengers, held.container).cloned() {
                    list.0.retain(|passenger| passenger != entity);
                    tick.state_mut()
                        .insert(ids.passengers, held.container, list)?;
                    self.garrison_follows_passengers(tick, ids, held.container)?;
                }
                tick.state_mut().remove(ids.contained, *entity);
            }
        }
        let mut victims: Vec<(Entity, f32)> = Vec::new();
        for (container, _) in killed {
            if self.is_tunnel(tick.state(), *container) {
                let heir = self
                    .tunnel_pool(tick.state(), ids, *container)
                    .into_iter()
                    .find(|tunnel| {
                        *tunnel != *container
                            && !killed.iter().any(|(dead, _)| dead == tunnel)
                            && tick.state().is_live(*tunnel)
                    });
                if let Some(heir) = heir {
                    self.move_passengers(tick, ids, *container, heir)?;
                    continue;
                }
            }
            let Some(list) = tick.state().get(ids.passengers, *container).cloned() else {
                continue;
            };
            let Some((damage, kill, eject)) =
                self.contain_view(tick.state(), *container).map(|view| {
                    (
                        view.open.damage_percent_to_units(),
                        view.open.kill_passengers_on_death(),
                        view.open.eject_passengers_on_death(),
                    )
                })
            else {
                continue;
            };
            for passenger in &list.0 {
                for entity in Self::object_and_members(tick.state(), ids, *passenger) {
                    let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
                        continue;
                    };
                    if damage > 0.0 {
                        let after = health.0.current() - health.0.max() * damage;
                        health.0.set_current(after.max(0.0));
                    }
                    if kill && !health.0.kill() {
                        health.0.set_current(0.0);
                    }
                    let dead = health.0.is_dead();
                    tick.state_mut().insert(ids.health, entity, health)?;
                    if dead && !killed.iter().any(|(already, _)| *already == entity) {
                        victims.push((entity, 0.0));
                    }
                }
            }
            if eject || !kill {
                self.schedule_exit(tick, ids, *container)?;
                for passenger in &list.0 {
                    if let Some(mut held) = tick.state().get(ids.contained, *passenger).copied() {
                        held.leaves_at = tick.frame();
                        tick.state_mut().insert(ids.contained, *passenger, held)?;
                    }
                }
                self.release_passengers(tick, ids)?;
            }
        }
        victims.sort_unstable_by_key(|(entity, _)| entity.index());
        victims.dedup_by_key(|(entity, _)| *entity);
        Ok(victims)
    }

    pub(super) fn controller_of(world: &World, ids: Ids, object: Entity) -> Option<PlayerId> {
        world.get(ids.owner, object).map(|held| held.0).or_else(|| {
            world
                .get(ids.horde, object)
                .and_then(|horde| horde.members.first().copied())
                .and_then(|member| world.get(ids.owner, member).map(|held| held.0))
        })
    }

    fn garrison_follows_passengers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
    ) -> Result<()> {
        let Some(mut list) = tick.state().get(ids.passengers, container).cloned() else {
            return Ok(());
        };
        let garrison = self
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.is_garrison(tick.state(), container));
        let current = tick.state().get(ids.owner, container).map(|held| held.0);
        let wanted = match list.0.first() {
            Some(first) if garrison => Self::controller_of(tick.state(), ids, *first),
            _ => list.1,
        };
        if list.0.is_empty() {
            tick.state_mut().remove(ids.passengers, container);
        } else if garrison && list.1.is_none() && wanted != current {
            list.1 = current;
            tick.state_mut().insert(ids.passengers, container, list)?;
        }
        let Some(wanted) = wanted.filter(|wanted| Some(*wanted) != current) else {
            return Ok(());
        };
        if self.change_owner_for(tick, ids, container, wanted, OwnerChange::Capture)? {
            diag!(
                "contain: garrison entity {} now controlled by player {}",
                container.index(),
                wanted.get()
            );
        }
        Ok(())
    }

    pub(crate) fn contained_status(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<ContainStatus> {
        world.get(ids.contained, entity).map(|held| held.status)
    }

    pub(super) fn turns_inside(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let Some(held) = world.get(ids.contained, entity) else {
            return false;
        };
        !held.status.has(ContainStatus::ENCLOSED)
            && self.modules.as_ref().is_some_and(|wiring| {
                wiring
                    .contain_view(world, held.container)
                    .is_some_and(|view| !view.open.forces_orientation())
            })
    }

    pub(super) fn first_passenger(
        &self,
        world: &World,
        ids: Ids,
        container: Entity,
    ) -> Option<Entity> {
        world.get(ids.passengers, container)?.0.first().copied()
    }

    pub(super) fn react_evacuate_damage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        weapon: Option<mint_sage_combat::WeaponId>,
    ) -> Result<()> {
        let Some(weapon) = weapon else {
            return Ok(());
        };
        let Some(kind) = tick.state().get(ids.kind, victim).copied() else {
            return Ok(());
        };
        if self.contain_damage.evacuate_weapon(kind) != Some(weapon) {
            return Ok(());
        }
        if tick
            .state()
            .get(ids.passengers, victim)
            .is_none_or(|list| list.0.is_empty())
        {
            return Ok(());
        }
        diag!(
            "contain: entity {} evacuated by EvacuateDamage's WeaponThatCausesEvacuation",
            victim.index()
        );
        self.schedule_exit(tick, ids, victim)
    }

    pub(super) fn react_horde_transport_contain_damage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        container: Entity,
    ) -> Result<()> {
        let Some(kind) = tick.state().get(ids.kind, container).copied() else {
            return Ok(());
        };
        if !self.contain_damage.has_transport_rider_damage(kind) {
            return Ok(());
        }
        let Some(health) = tick.state().get(ids.health, container).copied() else {
            return Ok(());
        };
        let max = health.0.max();
        if max <= 0.0 {
            return Ok(());
        }
        let current = health.0.current();
        let Some(passengers) = tick.state().get(ids.passengers, container).cloned() else {
            return Ok(());
        };
        let mut killed: Vec<(Entity, f32)> = Vec::new();
        for passenger in &passengers.0 {
            let Some(horde) = tick.state().get(ids.horde, *passenger).cloned() else {
                continue;
            };
            let full = horde.slots.len();
            let alive = horde.members.len();
            if full == 0 || alive <= 1 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let share = max / full as f32;
            if share <= 0.0 {
                continue;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let allowed = ((current / share).floor() + 1.0)
                .clamp(0.0, full as f32)
                .floor() as usize;
            if alive <= allowed {
                continue;
            }
            for &member in horde
                .members
                .iter()
                .rev()
                .take(alive.saturating_sub(allowed))
            {
                if let Some(mut member_health) = tick.state().get(ids.health, member).copied() {
                    member_health.0.set_current(0.0);
                    tick.state_mut().insert(ids.health, member, member_health)?;
                    killed.push((member, 0.0));
                }
            }
        }
        if !killed.is_empty() {
            diag!(
                "contain: entity {} HordeTransportContainDamage killed {} rider(s)",
                container.index(),
                killed.len()
            );
            self.resolve_deaths(tick, ids, &killed)?;
        }
        Ok(())
    }
}
