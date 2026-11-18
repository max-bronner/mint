use mint_sage_combat::{WeaponState, WeaponStatus};
use mint_sage_scene::WeaponPhase;
use mint_sim::{Frame, LogicFrames, World};

use crate::rules::Ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attack {
    None,
    Pursuing,
    Engaged,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ArmedView {
    pub(crate) slot: usize,
    pub(crate) state: WeaponState,
    pub(crate) firing: LogicFrames,
    pub(crate) attack_cycle: u32,
}

pub(crate) fn weapon_phase(weapon: ArmedView, attack: Attack, tick: Frame) -> Option<WeaponPhase> {
    if attack == Attack::None {
        return None;
    }
    let mut state = weapon.state;
    let status = state.status(tick);
    let shot = state.last_shot();
    let firing = shot.is_some_and(|shot| {
        tick.since(shot)
            .is_some_and(|elapsed| elapsed.get() < weapon.firing.get().max(1))
    });
    let shot_frame = shot == Some(tick);
    match status {
        WeaponStatus::PreAttack => Some(WeaponPhase::PreAttack),
        WeaponStatus::OutOfAmmo => None,
        WeaponStatus::ReloadingClip if !shot_frame => Some(WeaponPhase::Reloading),
        _ if firing => Some(WeaponPhase::Firing),
        WeaponStatus::BetweenFiringShots => Some(WeaponPhase::BetweenFiringShots),
        WeaponStatus::ReloadingClip => Some(WeaponPhase::Reloading),
        WeaponStatus::ReadyToFire => {
            (attack == Attack::Engaged).then_some(WeaponPhase::BetweenFiringShots)
        }
    }
}

pub(crate) fn attack_of(
    world: &World,
    ids: Ids,
    entity: mint_sim::Entity,
    now: Frame,
    firing_duration: &impl Fn(mint_sage_combat::WeaponId) -> LogicFrames,
) -> Attack {
    let armed = world.get(ids.armed, entity);
    if !world.contains(ids.attack_order, entity) && armed.is_none_or(|armed| armed.target.is_none())
    {
        let finishing = armed
            .is_some_and(|armed| swing_in_flight(world, ids, entity, armed, now, firing_duration));
        if finishing {
            Attack::Engaged
        } else {
            Attack::None
        }
    } else if armed.is_some_and(|armed| armed.winding || armed.shots_at_target > 0) {
        Attack::Engaged
    } else {
        Attack::Pursuing
    }
}

pub(crate) fn armed_view(
    world: &World,
    ids: Ids,
    entity: mint_sim::Entity,
    firing_duration: &impl Fn(mint_sage_combat::WeaponId) -> LogicFrames,
) -> Option<ArmedView> {
    world.get(ids.armed, entity).map(|armed| ArmedView {
        slot: world
            .get(ids.weapon_slots, entity)
            .map_or(0, |slots| usize::from(slots.current)),
        state: armed.state,
        firing: firing_duration(armed.weapon),
        attack_cycle: armed.state.attack_cycle(),
    })
}

pub(crate) fn using_weapon(
    world: &World,
    ids: Ids,
    entity: mint_sim::Entity,
    now: Frame,
    firing_duration: &impl Fn(mint_sage_combat::WeaponId) -> LogicFrames,
) -> bool {
    let attack = attack_of(world, ids, entity, now, firing_duration);
    let tick = Frame::new(now.get().saturating_sub(1));
    armed_view(world, ids, entity, firing_duration)
        .and_then(|weapon| weapon_phase(weapon, attack, tick))
        .is_some()
}

fn swing_in_flight(
    world: &World,
    ids: Ids,
    entity: mint_sim::Entity,
    armed: &crate::combat::Armed,
    now: Frame,
    firing_duration: &impl Fn(mint_sage_combat::WeaponId) -> LogicFrames,
) -> bool {
    let ordered = |mover: mint_sim::Entity| {
        world
            .get(ids.path, mover)
            .is_some_and(|path| path.kind == crate::unit::PathKind::Ordered)
    };
    let ordered_away = ordered(entity)
        || world
            .get(ids.member, entity)
            .is_some_and(|member| member.parked || ordered(member.horde));
    if ordered_away {
        return false;
    }
    let tick = Frame::new(now.get().saturating_sub(1));
    if tick < armed.state.when_pre_attack_finished() {
        return true;
    }
    armed.state.last_shot().is_some_and(|shot| {
        tick.since(shot)
            .is_some_and(|elapsed| elapsed.get() < firing_duration(armed.weapon).get().max(1))
    })
}
