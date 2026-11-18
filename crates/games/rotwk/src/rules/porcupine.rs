use mint_sage_ini::Stance;

use super::guard::GuardMode;
use super::movement::PORCUPINE;
use crate::unit::Kind;

use super::*;

impl RotwkRules {
    pub(super) fn toggle_horde_formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        let selected = self.selection(tick, ids, player);
        let mut hordes: Vec<Entity> = Vec::new();
        for entity in selected {
            let owned = tick
                .state()
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player);
            if !owned {
                continue;
            }
            let unit = Self::commanded_unit(tick.state(), ids, entity);
            if tick.state().contains(ids.horde, unit) {
                hordes.push(unit);
            }
        }
        hordes.sort_unstable_by_key(|entity| entity.index());
        hordes.dedup();
        for horde_entity in hordes {
            self.change_to_formation(tick, ids, horde_entity)?;
        }
        Ok(())
    }

    fn alternate_formation(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> Option<(Kind, u16)> {
        let kind = tick.state().get(ids.kind, horde_entity).copied()?;
        let member_count = tick.state().get(ids.horde, horde_entity)?.members.len();
        if member_count == 0 {
            return None;
        }
        let entry = self.kinds.get(kind)?;
        let table = self.formations.get(usize::from(entry.formation?))?;
        let name = table.settings().alternate_formation.as_deref()?;
        let alt_kind = self.kinds.kind(name)?;
        let alt_index = self.kinds.get(alt_kind)?.formation?;
        let alt_table = self.formations.get(usize::from(alt_index))?;
        let settings = alt_table.settings();
        #[allow(clippy::cast_possible_truncation)]
        let count = member_count as u32;
        if !settings.main_formation && count < settings.minimum_horde_size {
            return None;
        }
        Some((alt_kind, alt_index))
    }

    pub(super) fn change_to_formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> Result<bool> {
        let Some((alt_kind, alt_index)) = self.alternate_formation(tick, ids, horde_entity) else {
            return Ok(false);
        };
        let Some(alt_table) = self.formations.get(usize::from(alt_index)).cloned() else {
            return Ok(false);
        };
        let into_porcupine = alt_table.settings().porcupine;
        let now = tick.frame();
        if into_porcupine {
            tick.state_mut().remove(ids.deferred_order, horde_entity);
            Self::clear_path_retry(tick, ids, horde_entity);
            self.stop_unit(tick, ids, horde_entity);
        }
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(false);
        };
        let banner_member = horde.banner_slot.and_then(|slot| {
            horde.members.iter().copied().find(|member| {
                tick.state()
                    .get(ids.member, *member)
                    .is_some_and(|held| held.slot == Some(slot))
            })
        });
        let offsets = alt_table.place(tick.rng());
        let mut slots: Vec<crate::horde::HordeSlot> = offsets
            .iter()
            .zip(alt_table.slots())
            .map(|(offset, slot)| crate::horde::HordeSlot {
                offset: *offset,
                rank: slot.rank,
                unit_type: Some(slot.unit_type),
                index_in_rank: slot.index_in_rank,
                leader: slot.leader,
            })
            .collect();
        let new_banner_slot = banner_member.and_then(|_| {
            let index = u16::try_from(slots.len()).ok()?;
            slots.push(crate::horde::HordeSlot {
                offset: Vec2::ZERO,
                rank: 0,
                unit_type: None,
                index_in_rank: 0,
                leader: None,
            });
            Some(index)
        });
        horde.formation = alt_index;
        horde.slots = slots;
        horde.banner_slot = new_banner_slot;
        horde.dirty = true;
        for member in horde.members.clone() {
            let slot = if Some(member) == banner_member {
                new_banner_slot
            } else {
                None
            };
            tick.state_mut().insert(
                ids.member,
                member,
                crate::horde::Member {
                    horde: horde_entity,
                    slot,
                    parked: false,
                },
            )?;
        }
        horde.free = (0..u16::try_from(horde.slots.len()).unwrap_or(u16::MAX))
            .filter(|slot| Some(*slot) != horde.banner_slot)
            .collect();
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
        tick.state_mut().insert(ids.kind, horde_entity, alt_kind)?;
        crate::horde::reassign_slots(tick, Self::horde_ids(ids))?;
        if into_porcupine {
            self.change_stance(tick, ids, horde_entity, Stance::Porcupine)?;
            self.leave_guard(tick, ids, horde_entity);
            if let Some(guard) =
                self.guard_here(tick, ids, horde_entity, GuardMode::HoldGround, now)
            {
                tick.state_mut().insert(ids.guard, horde_entity, guard)?;
            }
        } else {
            self.change_stance(tick, ids, horde_entity, Stance::Battle)?;
            let members = tick
                .state()
                .get(ids.horde, horde_entity)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            let effects: Vec<_> = members
                .into_iter()
                .filter(|member| self.holds_model_condition(tick, *member, PORCUPINE))
                .map(|member| {
                    (
                        member,
                        mint_sage_modules::Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(Vec::new()),
                            remove: mint_sage_modules::effects::names(vec![PORCUPINE.to_owned()]),
                        },
                    )
                })
                .collect();
            if !effects.is_empty() {
                self.apply_effects(tick, ids, effects)?;
            }
        }
        Ok(true)
    }

    pub(super) fn is_porcupine_horde(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> bool {
        tick.state()
            .get(ids.stance, horde_entity)
            .is_some_and(|stance| stance.0 == Stance::Porcupine)
    }

    pub(super) fn porcupine_gate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        is_attack: bool,
    ) -> Result<bool> {
        if !self.is_porcupine_horde(tick, ids, horde_entity) {
            return Ok(true);
        }
        if is_attack {
            return Ok(true);
        }
        self.change_to_formation(tick, ids, horde_entity)
    }
}
