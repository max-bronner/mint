use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sim::{Entity, Result, StateHasher, Tick};

use super::area::Area;
use super::counters::TICKS_PER_SCRIPT_FRAME;
use super::orders::{ScriptOrder, HUNT_RANGE};
use super::MapScriptState;
use crate::rules::{Ids, RotwkRules};

const ATTACK_AREA_SCAN_FRAMES: u64 = 5;
const ATTACK_AREA_FIRST_SCAN_SPREAD: u32 = 6;

#[derive(Debug, Clone)]
struct Scan {
    area: Arc<Area>,
    next_scan: u64,
}

impl PartialEq for Scan {
    fn eq(&self, other: &Self) -> bool {
        self.area.name == other.area.name && self.next_scan == other.next_scan
    }
}

impl Eq for Scan {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AttackAreas(BTreeMap<Entity, Scan>);

impl AttackAreas {
    pub(super) fn begin(&mut self, entity: Entity, area: Arc<Area>, next_scan: u64) {
        self.0.insert(entity, Scan { area, next_scan });
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.0.remove(&entity);
    }

    pub(super) fn holds(&self, entity: Entity) -> bool {
        self.0.contains_key(&entity)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity, _| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (entity, scan) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_bytes(scan.area.name.as_bytes());
            hasher.write_u64(scan.next_scan);
        }
    }
}

impl RotwkRules {
    pub(super) fn script_frame(tick: &Tick<'_>) -> u64 {
        tick.frame().get() / TICKS_PER_SCRIPT_FRAME
    }

    pub(super) fn begin_attack_area(
        &self,
        tick: &mut Tick<'_>,
        state: &mut MapScriptState,
        members: &[Entity],
        area: &Arc<Area>,
    ) {
        let now = Self::script_frame(tick);
        for &member in members {
            let offset = u64::from(tick.rng().below(ATTACK_AREA_FIRST_SCAN_SPREAD));
            state.rt.walks.forget(member);
            state.rt.hunters.forget(member);
            state
                .rt
                .attack_areas
                .begin(member, Arc::clone(area), now + offset);
        }
    }

    pub(super) fn step_attack_areas(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let now = Self::script_frame(tick);
        let due: Vec<(Entity, Arc<Area>)> = state
            .rt
            .attack_areas
            .0
            .iter()
            .filter(|(_, scan)| scan.next_scan <= now)
            .map(|(&entity, scan)| (entity, Arc::clone(&scan.area)))
            .collect();
        for (member, area) in due {
            if let Some(scan) = state.rt.attack_areas.0.get_mut(&member) {
                scan.next_scan = now + ATTACK_AREA_SCAN_FRAMES;
            }
            let world = tick.state();
            if !world.is_live(member) {
                state.rt.attack_areas.forget(member);
                continue;
            }
            let Some(at) = self.position_of_entity(tick, ids, member) else {
                state.rt.attack_areas.forget(member);
                continue;
            };
            if self.out_of_ammo(tick, ids, member) {
                state.rt.attack_areas.forget(member);
                self.issue_script_order(tick, ids, &[member], ScriptOrder::Stop)?;
                continue;
            }
            let foes: Vec<_> = self
                .hostile_foes(tick, ids, member)
                .into_iter()
                .filter(|foe| {
                    !foe.unattackable
                        && area.contains(foe.position)
                        && !self.off_map(tick.state(), ids, foe.position)
                })
                .collect();
            let radius = Self::mover_radius(tick, ids, member);
            let victim = self.acquire_for(world, ids, member, at, radius, &foes, 0.0, HUNT_RANGE);
            let victim = victim.map(|victim| self.horde_member_pick(tick, ids, member, victim));
            let Some(victim) = victim else {
                state.rt.attack_areas.forget(member);
                self.issue_script_order(tick, ids, &[member], ScriptOrder::Stop)?;
                continue;
            };
            let world = tick.state();
            if !world.contains(ids.path, member) && !world.contains(ids.attack_order, member) {
                self.issue_script_order(tick, ids, &[member], ScriptOrder::Attack(victim))?;
            }
        }
        Ok(())
    }
}
