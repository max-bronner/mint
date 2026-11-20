use super::*;
use crate::combat::{target_allowed, Armed, Foe, IdleScan};

pub(super) struct LastShotScan<'a> {
    pub(super) entity: Entity,
    pub(super) me: &'a Foe,
    pub(super) player: PlayerId,
    pub(super) scan: &'a IdleScan,
    pub(super) anti_mask: mint_sage_combat::AntiMask,
    pub(super) reach_gated: bool,
    pub(super) stands: bool,
}

impl RotwkRules {
    pub(super) fn note_shot_horde(world: &World, ids: Ids, state: &mut Armed, victim: Entity) {
        if let Some(member) = world.get(ids.member, victim) {
            state.last_shot_horde = Some(member.horde);
        }
    }

    pub(super) fn take_last_shot_pick(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut Armed,
        living: &[(Foe, PlayerId)],
        scanner: &LastShotScan<'_>,
    ) -> Option<Foe> {
        let horde = state.last_shot_horde.take()?;
        let first = *tick.state().get(ids.horde, horde)?.members.first()?;
        let picked = self.horde_member_pick(tick, ids, scanner.entity, first);
        let at = living
            .binary_search_by_key(&picked, |(foe, _)| foe.entity)
            .ok()?;
        let picked = living.get(at).map(|(foe, _)| *foe)?;
        let me = scanner.me;
        let (dx, dy) = (
            picked.position.x - me.position.x,
            picked.position.y - me.position.y,
        );
        let reach = scanner.scan.reach;
        if dx * dx + dy * dy > reach * reach {
            return None;
        }
        let world = tick.state();
        let relation = self
            .relation_from(world, scanner.entity)
            .to(picked.entity)
            .victim();
        let high = self.active_high(world, ids);
        let allowed = crate::combat::auto_acquirable(relation)
            && mint_sage_combat::can_target(
                mint_sage_combat::Attacker {
                    relation,
                    class_allowed: target_allowed(scanner.anti_mask, &picked),
                    scans_structures: scanner.scan.structures,
                },
                self.scanned_victim(world, ids, scanner.player, &picked),
                mint_sage_combat::Mode::AutoAcquire,
            )
            && !super::targeting::outside(high, picked.position)
            && (!scanner.reach_gated
                || self.attackable_from_here(
                    tick,
                    ids,
                    scanner.entity,
                    me.position,
                    me.radius,
                    &picked,
                    scanner.stands,
                ));
        allowed.then_some(picked)
    }
}
