use mint_movement::{Facing, Vec2};
use mint_sage_formation::FormationBasis;
use mint_sage_modules::effects::{Effect, Name};
use mint_sim::module::UpgradeMask;
use mint_sim::{
    Component, Entity, Frame, LogicFrames, ModuleFieldSource, PlayerId, Result, StateHasher, Tick,
};

use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::horde::Horde;
use crate::unit::Kind;

const BANNER_RESPAWN_QUIET: LogicFrames = LogicFrames::from_milliseconds(4000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HordeCombat(pub(crate) Frame);

impl Component for HordeCombat {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HordeLastShot(pub(crate) Frame);

impl Component for HordeLastShot {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Banner {
    pub(crate) slot: u16,
    pub(crate) kind: Kind,
    pub(crate) lost_at: Option<Frame>,
}

impl Component for Banner {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.slot);
        self.kind.hash_into(hasher);
        match self.lost_at {
            Some(frame) => {
                hasher.write_u8(1);
                frame.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

pub(crate) struct Replenishment<'a> {
    pub(crate) melee_free: LogicFrames,
    pub(crate) fx: Option<&'a Name>,
    pub(crate) nearby: Option<f32>,
    pub(crate) all_nearby: bool,
    pub(crate) upgrade_required: &'a UpgradeMask,
}

impl RotwkRules {
    pub(super) fn note_horde_combat(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        let now = tick.frame();
        let Some(horde) = tick
            .state()
            .get(ids.member, entity)
            .map(|member| member.horde)
        else {
            return;
        };
        if !tick.state().contains(ids.horde, horde) {
            return;
        }
        let _ = tick
            .state_mut()
            .insert(ids.horde_combat, horde, HordeCombat(now));
    }

    pub(super) fn note_horde_last_shot(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        let now = tick.frame();
        let Some(horde) = tick
            .state()
            .get(ids.member, entity)
            .map(|member| member.horde)
        else {
            return;
        };
        if !tick.state().contains(ids.horde, horde) {
            return;
        }
        let _ = tick
            .state_mut()
            .insert(ids.horde_last_shot, horde, HordeLastShot(now));
    }

    pub(super) fn replenish_hordes(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        pulse: &Replenishment<'_>,
    ) -> Result<()> {
        if !pulse.upgrade_required.is_empty()
            && !self
                .effective_upgrades_in(tick.state(), ids, source)
                .contains_all(pulse.upgrade_required)
        {
            return Ok(());
        }
        if tick.state().contains(ids.construction, source) {
            return Ok(());
        }
        if self.holds_condition(tick, source, "BURNINGDEATH") {
            return Ok(());
        }
        if self.modules.as_ref().is_some_and(|wiring| {
            wiring.carries_status(tick.state(), source, "TEMPORARILY_DEFECTED")
        }) {
            return Ok(());
        }
        if super::upgrades::is_moving(tick, ids, source) {
            return Ok(());
        }
        if self.has_negative_emotion(tick.state(), ids, source) {
            return Ok(());
        }
        let Some(radius) = pulse.nearby else {
            let Some(horde) = tick
                .state()
                .get(ids.member, source)
                .map(|member| member.horde)
            else {
                return Ok(());
            };
            if tick.state().contains(ids.attack_order, horde) {
                return Ok(());
            }
            self.regrow_one(tick, ids, horde, pulse, true)?;
            return Ok(());
        };
        for horde in self.hordes_within(tick, ids, source, radius) {
            if self.regrow_one(tick, ids, horde, pulse, false)? && !pulse.all_nearby {
                break;
            }
        }
        Ok(())
    }

    fn hordes_within(&self, tick: &Tick<'_>, ids: Ids, source: Entity, radius: f32) -> Vec<Entity> {
        let world = tick.state();
        let (Some(owner), Some(at)) = (
            world.get(ids.owner, source).map(|owner| owner.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.horde) else {
            return Vec::new();
        };
        let mut found: Vec<(f32, u32, Entity)> = store
            .iter()
            .filter_map(|(horde, members)| {
                let position = world.get(ids.body, horde).map(|body| body.0.position)?;
                let member = members.members.first().copied()?;
                if world.get(ids.owner, member).map(|owner| owner.0) != Some(owner) {
                    return None;
                }
                let away = position.minus(at);
                let distance = away.x * away.x + away.y * away.y;
                (distance <= radius * radius).then_some((distance, horde.index(), horde))
            })
            .collect();
        found.sort_unstable_by(|a, b| {
            a.0.to_bits()
                .cmp(&b.0.to_bits())
                .then_with(|| a.1.cmp(&b.1))
        });
        found.into_iter().map(|(_, _, horde)| horde).collect()
    }

    fn regrow_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        pulse: &Replenishment<'_>,
        melee_gated: bool,
    ) -> Result<bool> {
        if melee_gated && self.fought_within(tick, ids, horde_entity, pulse.melee_free) {
            return Ok(false);
        }
        if melee_gated && self.garrison_blocks_regrowth(tick, ids, horde_entity) {
            return Ok(false);
        }
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(false);
        };
        if horde.free.is_empty() {
            return Ok(false);
        }
        let drawn = tick
            .rng()
            .below(u32::try_from(horde.free.len()).unwrap_or(u32::MAX));
        let Some(kind) = horde
            .free
            .get(usize::try_from(drawn).unwrap_or(0))
            .copied()
            .and_then(|slot| self.slot_kind(&horde, slot))
        else {
            return Ok(false);
        };
        let Some(slot) = self.free_slot(&horde, kind) else {
            return Ok(false);
        };
        let Some(owner) = horde
            .members
            .iter()
            .find_map(|member| tick.state().get(ids.owner, *member).map(|owner| owner.0))
        else {
            return Ok(false);
        };
        let Some(spawned) = self.grow_into_slot(tick, ids, horde_entity, kind, slot, owner)? else {
            return Ok(false);
        };
        diag!(
            "replenish: horde {} regained a member in slot {slot} at frame {}",
            horde_entity.index(),
            tick.frame().get()
        );
        if let (Some(list), Some(at)) = (
            pulse.fx,
            tick.state().get(ids.body, spawned).map(|b| b.0.position),
        ) {
            self.emit_fx_object_named(
                tick,
                ids,
                list,
                at,
                crate::fx::FxSource::Power,
                spawned,
                None,
            );
        }
        Ok(true)
    }

    fn fought_within(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        window: LogicFrames,
    ) -> bool {
        let now = tick.frame();
        tick.state()
            .get(ids.horde_combat, horde_entity)
            .is_some_and(|last| {
                last.0
                    .checked_add(window)
                    .is_none_or(|free_at| now < free_at)
            })
    }

    fn shot_within(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        window: LogicFrames,
    ) -> bool {
        let now = tick.frame();
        tick.state()
            .get(ids.horde_last_shot, horde_entity)
            .is_some_and(|last| {
                last.0
                    .checked_add(window)
                    .is_none_or(|free_at| now < free_at)
            })
    }

    fn garrison_blocks_regrowth(&self, tick: &Tick<'_>, ids: Ids, horde_entity: Entity) -> bool {
        let Some(container) = tick
            .state()
            .get(ids.contained, horde_entity)
            .map(|contained| contained.container)
        else {
            return false;
        };
        if !self.has_kind_bit(tick.state(), ids, container, "GARRISON") {
            return false;
        }
        let Some(module) = tick
            .state()
            .get(ids.kind, container)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| {
                entry
                    .record
                    .declared_modules
                    .iter()
                    .rev()
                    .find(|module| module.name.eq_ignore_ascii_case("AllowBannerSpawnUpgrade"))
            })
        else {
            return false;
        };
        let trigger = module
            .source(&self.upgrade_bits)
            .field_upgrade_mask("TriggeredBy");
        !trigger.is_empty()
            && !self
                .effective_upgrades_in(tick.state(), ids, container)
                .contains_all(&trigger)
    }

    pub(super) fn reform_horde(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> Result<()> {
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let Some(table) = self.formations.get(usize::from(horde.formation)) else {
            return Ok(());
        };
        let kinds: Vec<Option<Kind>> = horde
            .slots
            .iter()
            .map(|slot| {
                slot.unit_type
                    .and_then(|unit_type| table.unit_types().get(usize::from(unit_type)))
                    .and_then(|name| self.kinds.kind(name))
            })
            .collect();
        let member_kinds: Vec<(Entity, Option<Kind>)> = horde
            .members
            .iter()
            .map(|member| (*member, tick.state().get(ids.kind, *member).copied()))
            .collect();
        let fits = |member: Entity, slot: u16| {
            let theirs = member_kinds
                .iter()
                .find(|(m, _)| *m == member)
                .and_then(|(_, kind)| *kind);
            kinds
                .get(usize::from(slot))
                .copied()
                .flatten()
                .is_some_and(|kind| Some(kind) == theirs)
        };
        let stop_ranks = table.settings().ranks_that_stop_advance.clone();
        let is_aod = table.settings().aod.is_some();
        crate::horde::reform(
            tick,
            Self::horde_ids(ids),
            horde_entity,
            &fits,
            &stop_ranks,
            is_aod,
        )
    }

    fn free_slot(&self, horde: &Horde, kind: Kind) -> Option<u16> {
        horde
            .free
            .iter()
            .copied()
            .find(|free| self.slot_kind(horde, *free) == Some(kind))
    }

    fn slot_kind(&self, horde: &Horde, slot: u16) -> Option<Kind> {
        let table = self.formations.get(usize::from(horde.formation))?;
        horde
            .slots
            .get(usize::from(slot))
            .and_then(|slot| slot.unit_type)
            .and_then(|unit_type| table.unit_types().get(usize::from(unit_type)))
            .and_then(|name| self.kinds.kind(name))
    }

    fn grow_into_slot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        kind: Kind,
        slot: u16,
        owner: PlayerId,
    ) -> Result<Option<Entity>> {
        let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
            return Ok(None);
        };
        let Some(current) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(None);
        };
        let Some(offset) = current.offset(slot) else {
            return Ok(None);
        };
        let is_aod = self
            .formations
            .get(usize::from(current.formation))
            .is_some_and(|table| table.settings().aod.is_some());
        let facing = body.0.facing;
        let pose = self.standing_pose(body.0.position, facing, offset);
        let Some(spawned) = self.spawn_kind_at(tick, ids, kind, pose, facing, owner)? else {
            return Ok(None);
        };
        tick.state_mut().insert(
            ids.member,
            spawned,
            crate::horde::Member {
                horde: horde_entity,
                slot: Some(slot),
                parked: false,
            },
        )?;
        if is_aod {
            let now = tick.frame();
            tick.state_mut().insert(
                ids.join_fade,
                spawned,
                crate::horde::JoinFade::starting(now, crate::horde::JOIN_FADE_DURATION),
            )?;
        }
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(Some(spawned));
        };
        horde.members.push(spawned);
        horde.free.retain(|free| *free != slot);
        horde.dirty = true;
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
        self.propagate_rank(tick, ids, horde_entity)?;
        Ok(Some(spawned))
    }

    fn standing_pose(&self, origin: Vec2, facing: Facing, offset: Vec2) -> Vec2 {
        crate::skirmish::standable(
            &self.passability,
            FormationBasis::of(facing).place(origin, offset),
        )
    }

    pub(super) fn note_banner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        banner: Entity,
        kind: Kind,
    ) -> Result<()> {
        let Some(slot) = tick
            .state()
            .get(ids.member, banner)
            .and_then(|member| member.slot)
        else {
            return Ok(());
        };
        tick.state_mut().insert(
            ids.banner,
            horde_entity,
            Banner {
                slot,
                kind,
                lost_at: None,
            },
        )?;
        diag!(
            "replenish: horde {} banner carrier {} holds slot {slot}",
            horde_entity.index(),
            banner.index()
        );
        if let Some(member) = self.member_kind(tick, ids, horde_entity, kind) {
            self.wear_banner_morph(tick, ids, banner, member)?;
        }
        Ok(())
    }

    fn member_kind(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        banner_kind: Kind,
    ) -> Option<Kind> {
        let horde = tick.state().get(ids.horde, horde_entity)?;
        horde.members.iter().find_map(|member| {
            tick.state()
                .get(ids.kind, *member)
                .copied()
                .filter(|kind| *kind != banner_kind)
        })
    }

    pub(super) fn respawn_banners(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let hordes: Vec<(Entity, Banner)> = tick
            .state()
            .store(ids.banner)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, banner)| (entity, *banner))
                    .collect()
            })
            .unwrap_or_default();
        let now = tick.frame();
        for (horde_entity, mut banner) in hordes {
            if !tick.state().contains(ids.horde, horde_entity) {
                continue;
            }
            if self.slot_is_held(tick, ids, horde_entity, banner.slot) {
                if banner.lost_at.is_some() {
                    banner.lost_at = None;
                    tick.state_mut().insert(ids.banner, horde_entity, banner)?;
                }
                continue;
            }
            let Some(lost_at) = banner.lost_at else {
                banner.lost_at = Some(now);
                tick.state_mut().insert(ids.banner, horde_entity, banner)?;
                continue;
            };
            let Some(clocks) = self.banner_clocks(banner.kind) else {
                continue;
            };
            if lost_at
                .checked_add(clocks.died_respawn)
                .is_some_and(|due| now < due)
                || self.shot_within(tick, ids, horde_entity, BANNER_RESPAWN_QUIET)
            {
                continue;
            }
            let Some(owner) = tick
                .state()
                .get(ids.horde, horde_entity)
                .and_then(|horde| horde.members.first().copied())
                .and_then(|member| tick.state().get(ids.owner, member).map(|owner| owner.0))
            else {
                continue;
            };
            if let Some(carrier) =
                self.grow_into_slot(tick, ids, horde_entity, banner.kind, banner.slot, owner)?
            {
                if let Some(member) = self.member_kind(tick, ids, horde_entity, banner.kind) {
                    self.wear_banner_morph(tick, ids, carrier, member)?;
                }
                diag!(
                    "replenish: horde {} has a banner carrier again at frame {}",
                    horde_entity.index(),
                    now.get()
                );
                banner.lost_at = None;
                tick.state_mut().insert(ids.banner, horde_entity, banner)?;
            }
        }
        Ok(())
    }

    pub(super) fn banner_deaths(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[(Entity, f32)],
    ) -> Result<Vec<(Entity, f32)>> {
        let mut victims: Vec<(Entity, f32)> = Vec::new();
        for (entity, _) in killed {
            let Some(member) = tick.state().get(ids.member, *entity).copied() else {
                continue;
            };
            let horde_entity = member.horde;
            let Some(slot) = member.slot else {
                continue;
            };
            let Some(banner) = tick.state().get(ids.banner, horde_entity).copied() else {
                continue;
            };
            if banner.slot != slot {
                continue;
            }
            let destroys = tick
                .state()
                .get(ids.horde, horde_entity)
                .and_then(|horde| self.banners.get(usize::from(horde.formation)))
                .copied()
                .flatten()
                .is_some_and(|plan| plan.destroy_horde_on_death);
            if !destroys {
                continue;
            }
            let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
                continue;
            };
            for member_entity in &horde.members {
                if killed.iter().any(|(already, _)| already == member_entity)
                    || victims.iter().any(|(already, _)| already == member_entity)
                {
                    continue;
                }
                let Some(mut health) = tick.state().get(ids.health, *member_entity).copied() else {
                    continue;
                };
                if health.0.is_dead() {
                    continue;
                }
                if !health.0.kill() {
                    health.0.set_current(0.0);
                }
                tick.state_mut()
                    .insert(ids.health, *member_entity, health)?;
                victims.push((*member_entity, 0.0));
            }
        }
        Ok(victims)
    }

    fn slot_is_held(&self, tick: &Tick<'_>, ids: Ids, horde_entity: Entity, slot: u16) -> bool {
        tick.state()
            .get(ids.horde, horde_entity)
            .is_some_and(|horde| {
                horde.members.iter().any(|member| {
                    tick.state()
                        .get(ids.member, *member)
                        .is_some_and(|member| member.slot == Some(slot))
                })
            })
    }

    fn banner_clocks(&self, kind: Kind) -> Option<mint_sage_modules::BannerClocks> {
        self.banner_module(kind).map(|banner| banner.clocks())
    }

    fn banner_module(&self, kind: Kind) -> Option<mint_sage_modules::BannerCarrierUpdate> {
        let entry = self.kinds.get(kind)?;
        let module = entry
            .record
            .declared_modules
            .iter()
            .rev()
            .find(|module| module.name.eq_ignore_ascii_case("BannerCarrierUpdate"))?;
        mint_sage_modules::banner_carrier_update_from_fields(
            &module.source(&self.upgrade_bits),
            Frame::ZERO,
        )
        .ok()
    }

    fn wear_banner_morph(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        carrier: Entity,
        member: Kind,
    ) -> Result<()> {
        let Some(carrier_kind) = tick.state().get(ids.kind, carrier).copied() else {
            return Ok(());
        };
        let Some(module) = self.banner_module(carrier_kind) else {
            return Ok(());
        };
        let mut add: Vec<String> = Vec::new();
        let mut remove: Vec<String> = Vec::new();
        for morph in module.morphs() {
            let target = if self.kinds.kind(&morph.unit_type) == Some(member) {
                &mut add
            } else {
                &mut remove
            };
            target.extend(morph.conditions.iter().map(|flag| flag.to_string()));
        }
        if add.is_empty() {
            return Ok(());
        }
        remove.retain(|flag| !add.contains(flag));
        self.apply_effects(
            tick,
            ids,
            vec![(
                carrier,
                Effect::ModelConditions {
                    add: mint_sage_modules::effects::names(add),
                    remove: mint_sage_modules::effects::names(remove),
                },
            )],
        )?;
        if let (Some(list), Some(at)) = (
            module.morph_fx(),
            tick.state().get(ids.body, carrier).map(|b| b.0.position),
        ) {
            self.emit_fx_object_named(
                tick,
                ids,
                list,
                at,
                crate::fx::FxSource::Power,
                carrier,
                None,
            );
        }
        diag!(
            "replenish: banner carrier {} wears its {} look",
            carrier.index(),
            self.kinds
                .get(member)
                .map_or("", |entry| entry.record.name.as_str())
        );
        Ok(())
    }
}
