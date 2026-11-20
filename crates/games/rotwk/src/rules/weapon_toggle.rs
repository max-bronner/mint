use mint_sage_combat::{WeaponState, WeaponStatus, SLOT_COUNT};
use mint_sage_modules::Effect;

use super::upgrades::SlotSet;
use super::*;
use crate::combat::WeaponSlots;

const UNSET: u8 = u8::MAX;

fn next_toggle_slot([first, second, third]: [u8; 3], current: u8) -> Option<u8> {
    let or_first = |slot: u8| if slot == UNSET { first } else { slot };
    let next = if current == first {
        or_first(second)
    } else if current == second {
        or_first(third)
    } else if current == third {
        first
    } else {
        return None;
    };
    (next != UNSET).then_some(next)
}

impl RotwkRules {
    pub(super) fn toggle_weapon(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        toggles: [u8; 3],
        lead: Option<Entity>,
    ) -> Result<()> {
        let mut units: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .map(|entity| Self::commanded_unit(tick.state(), ids, entity))
            .collect();
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        let lead = lead
            .map(|lead| Self::commanded_unit(tick.state(), ids, lead))
            .or_else(|| units.first().copied());
        let Some(next) = lead
            .and_then(|lead| {
                let (_, slots) = self.selected_set(tick, ids, lead)?;
                Self::slot_in_use(tick.state(), ids, lead, &slots)
            })
            .and_then(|current| u8::try_from(current).ok())
            .and_then(|current| next_toggle_slot(toggles, current))
        else {
            return Ok(());
        };
        for unit in units {
            self.lock_toggled_slot(tick, ids, unit, next)?;
        }
        Ok(())
    }

    fn slot_in_use(world: &World, ids: Ids, entity: Entity, slots: &SlotSet) -> Option<usize> {
        match world.get(ids.weapon_slots, entity) {
            Some(held) => Some(usize::from(held.current)),
            None => {
                let weapon = world.get(ids.armed, entity)?.weapon;
                slots
                    .arms
                    .iter()
                    .position(|arm| arm.is_some_and(|arm| arm.weapon == weapon))
            }
        }
    }

    fn lock_toggled_slot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        next: u8,
    ) -> Result<()> {
        let Some((at, slots)) = self.selected_set(tick, ids, unit) else {
            return Ok(());
        };
        let Some(current) = Self::slot_in_use(tick.state(), ids, unit, &slots) else {
            return Ok(());
        };
        let target = usize::from(next);
        if target == current || slots.arms.get(target).copied().flatten().is_none() {
            return Ok(());
        }
        self.share_timers(tick, ids, unit, (at, &slots), (current, target))?;
        self.apply_effects(
            tick,
            ids,
            vec![
                (unit, Effect::ReleaseWeaponLock),
                (unit, Effect::LockWeaponSlot(next)),
            ],
        )
    }

    fn share_timers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        (at, slots): (usize, &SlotSet),
        (current, target): (usize, usize),
    ) -> Result<()> {
        let Some((weapon, mut from)) = tick
            .state()
            .get(ids.armed, unit)
            .map(|armed| (armed.weapon, armed.state))
        else {
            return Ok(());
        };
        let shares = self
            .combat
            .weapons
            .get(weapon)
            .is_some_and(|template| template.share_timers);
        if !shares || from.status(tick.frame()) == WeaponStatus::ReadyToFire {
            return Ok(());
        }
        let Some(incoming) = slots
            .arms
            .get(target)
            .copied()
            .flatten()
            .and_then(|arm| self.combat.weapons.get(arm.weapon))
        else {
            return Ok(());
        };
        let held = tick.state().get(ids.weapon_slots, unit).copied();
        let mut states = held.map_or([None; SLOT_COUNT], |held| held.states);
        let mut taken = states
            .get(target)
            .copied()
            .flatten()
            .unwrap_or_else(|| WeaponState::new(incoming));
        taken.take_next_fire_from(&from);
        if let Some(entry) = states.get_mut(target) {
            *entry = Some(taken);
        }
        let (Ok(set), Ok(current)) = (u8::try_from(at), u8::try_from(current)) else {
            return Ok(());
        };
        tick.state_mut().insert(
            ids.weapon_slots,
            unit,
            WeaponSlots {
                set,
                current,
                states,
            },
        )?;
        Ok(())
    }
}
