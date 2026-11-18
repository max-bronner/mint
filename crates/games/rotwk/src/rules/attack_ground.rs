use super::shooting::stand_off;
use super::*;
use crate::combat::GroundOrder;
use mint_sage_combat::{ShotOutcome, WeaponSlot};

fn position_reach(reach: f32) -> f32 {
    (reach - mint_sage_combat::weapon::RANGE_TRIM).max(0.0)
}

enum GroundWeapon {
    Ready(WeaponId),
    Missing,
    NeedsVictim(WeaponId),
}

impl RotwkRules {
    pub(super) fn attack_ground(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        slot: u8,
        at: Vec2,
    ) -> Result<()> {
        let Some(slot_name) = WeaponSlot::from_index(slot.into()).map(WeaponSlot::name) else {
            return Ok(());
        };
        if !at.x.is_finite() || !at.y.is_finite() {
            return Ok(());
        }
        let formations_on = self.modes(tick, ids).formation == FormationMode::On;
        let owned: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .collect();
        let hordes = if formations_on {
            crate::horde::hordes_of(tick.state(), Self::horde_ids(ids), &owned)
        } else {
            Vec::new()
        };
        let loose = owned.into_iter().filter(|entity| {
            tick.state().contains(ids.armed, *entity)
                && !tick
                    .state()
                    .get(ids.member, *entity)
                    .is_some_and(|member| hordes.contains(&member.horde))
        });
        let ordered: Vec<Entity> = hordes.iter().copied().chain(loose).collect();
        for entity in ordered {
            if tick.state().contains(ids.no_attack, entity)
                || Self::emotion_refuses_order(tick.state(), ids, entity)
                || self.siege_refuses_order(tick.state(), ids, entity)
            {
                continue;
            }
            if tick
                .state()
                .get(ids.ground_order, entity)
                .is_some_and(|order| {
                    order.slot == slot
                        && (order.at.x - at.x).abs() < 1e-4
                        && (order.at.y - at.y).abs() < 1e-4
                })
            {
                continue;
            }
            match self.ground_weapon(tick, ids, entity, slot) {
                GroundWeapon::Ready(_) => {}
                GroundWeapon::Missing => {
                    crate::diag::diag!(
                        "order: entity {} refuses AttackGround: no {slot_name} weapon",
                        entity.index()
                    );
                    continue;
                }
                GroundWeapon::NeedsVictim(weapon) => {
                    crate::diag::diag!(
                        "order: entity {} refuses AttackGround: {slot_name} weapon {} needs a victim (no NoVictimNeeded)",
                        entity.index(),
                        weapon.index()
                    );
                    continue;
                }
            }
            if !self.take_player_order(tick, ids, entity, false, true, true)? {
                continue;
            }
            let was_attacking = self.is_attacking_self_or_horde(tick, ids, entity);
            tick.state_mut().remove(ids.path, entity);
            let members = tick
                .state()
                .get(ids.horde, entity)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for member in members {
                tick.state_mut().remove(ids.path, member);
            }
            tick.state_mut()
                .insert(ids.ground_order, entity, GroundOrder { slot, at })?;
            crate::diag::diag!(
                "order: entity {} fires {slot_name} at ({:.0}, {:.0})",
                entity.index(),
                at.x,
                at.y
            );
            self.emit_voice_with(
                tick,
                ids,
                entity,
                crate::voice::VoiceKind::Attack,
                None,
                was_attacking,
            );
        }
        Ok(())
    }

    pub(super) fn ground_order_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        formations_on: bool,
    ) -> Option<GroundOrder> {
        tick.state()
            .get(ids.ground_order, entity)
            .copied()
            .or_else(|| {
                if !formations_on {
                    return None;
                }
                let member = tick.state().get(ids.member, entity)?;
                let horde_entity = member.horde;
                let order = tick.state().get(ids.ground_order, horde_entity).copied()?;
                if self.is_melee_horde(tick.state(), ids, horde_entity) {
                    return None;
                }
                let horde = tick.state().get(ids.horde, horde_entity)?;
                let rank = horde.slots.get(usize::from(member.slot?))?.rank;
                let table = self.formations.get(usize::from(horde.formation))?;
                table
                    .settings()
                    .ranks_to_release
                    .contains(&rank)
                    .then_some(order)
            })
    }

    fn ground_weapon(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, slot: u8) -> GroundWeapon {
        let arm = self
            .selected_set(tick, ids, entity)
            .and_then(|(_, slots)| slots.arms.get(usize::from(slot)).copied().flatten());
        let Some(arm) = arm else {
            return GroundWeapon::Missing;
        };
        if self
            .combat
            .weapons
            .get(arm.weapon)
            .is_some_and(|template| template.no_victim_needed)
        {
            GroundWeapon::Ready(arm.weapon)
        } else {
            GroundWeapon::NeedsVictim(arm.weapon)
        }
    }

    pub(super) fn approach_ground(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        order: GroundOrder,
        formations_on: bool,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let ranges = match self.ground_weapon(tick, ids, horde, order.slot) {
            GroundWeapon::Ready(weapon) => self.combat.weapons.get(weapon).map(|template| {
                (
                    self.min_range_of(tick.state(), horde, template),
                    position_reach(self.attack_range_of(tick.state(), ids, horde, template)),
                )
            }),
            GroundWeapon::Missing | GroundWeapon::NeedsVictim(_) => None,
        };
        let (Some((min_range, range)), Some(body), true) = (
            ranges,
            tick.state().get(ids.body, horde).copied(),
            formations_on,
        ) else {
            tick.state_mut().remove(ids.ground_order, horde);
            self.end_group_closing(tick, ids, horde);
            return Ok(());
        };
        let goal = match stand_off(body.0.position, order.at, 0.0, (min_range, range)) {
            Some(spot) => {
                self.los_search_goal(
                    tick,
                    ids,
                    horde,
                    super::shooting::AttackSpot::Straight,
                    body.0.position,
                    Self::mover_radius(tick, ids, horde),
                    order.at,
                    0.0,
                    None,
                    (min_range, range),
                    budgets,
                    (spot, 0.0),
                )
                .0
            }
            None if body.0.position.minus(order.at).length_squared() <= range * range => {
                self.end_group_closing(tick, ids, horde);
                return Ok(());
            }
            None => order.at,
        };
        self.close_on_target(
            tick,
            ids,
            horde,
            body.0.position,
            goal,
            0.0,
            PathKind::Closing,
            budgets,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fire_at_ground(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        mut state: Armed,
        order: GroundOrder,
        me: &Foe,
        living: &[(Foe, PlayerId)],
        budgets: &mut PathBudgets,
        events: &mut Vec<DamageEvent>,
    ) -> Result<()> {
        let own_order = tick.state().contains(ids.ground_order, entity);
        state.retarget(None);
        let GroundWeapon::Ready(weapon) = self.ground_weapon(tick, ids, entity, order.slot) else {
            if own_order {
                tick.state_mut().remove(ids.ground_order, entity);
            }
            tick.state_mut().insert(ids.armed, entity, state)?;
            return Ok(());
        };
        let Some(template) = self.combat.weapons.get(weapon) else {
            return Ok(());
        };
        if let Some(set) = self.selected_set(tick, ids, entity) {
            self.switch_slot(tick, ids, entity, &mut state, set, usize::from(order.slot))?;
        }
        let reach = position_reach(self.attack_range_of(tick.state(), ids, entity, template));
        let minimum = self.min_range_of(tick.state(), entity, template);
        let within = |min: f32| in_range(me.position, me.radius, order.at, 0.0, min, reach);
        if !within(minimum) {
            if own_order {
                let ranges = (minimum, reach);
                let (goal, goal_reach) = match stand_off(me.position, order.at, me.radius, ranges) {
                    Some(spot) => self.los_search_goal(
                        tick,
                        ids,
                        entity,
                        super::shooting::AttackSpot::InRange,
                        me.position,
                        me.radius,
                        order.at,
                        0.0,
                        None,
                        ranges,
                        budgets,
                        (spot, 0.0),
                    ),
                    None => (order.at, 0.0),
                };
                self.close_on_target(
                    tick,
                    ids,
                    entity,
                    me.position,
                    goal,
                    goal_reach,
                    PathKind::Closing,
                    budgets,
                )?;
            }
            tick.state_mut().insert(ids.armed, entity, state)?;
            return Ok(());
        }
        self.leave_ranged_approach(tick, ids, entity, Some(order.at))?;
        self.stop_to_fire(tick, ids, entity)?;
        let now = tick.frame();
        match super::shooting::after_firing(template, &state, now) {
            super::shooting::AfterFiring::Holding => {
                tick.state_mut().insert(ids.armed, entity, state)?;
                return Ok(());
            }
            super::shooting::AfterFiring::EndsAttack => {
                if own_order {
                    tick.state_mut().remove(ids.ground_order, entity);
                }
                tick.state_mut().insert(ids.armed, entity, state)?;
                return Ok(());
            }
            super::shooting::AfterFiring::Free => {}
        }
        let rate = tick
            .state()
            .get(ids.bonuses, entity)
            .map_or(1.0, |bonuses| bonuses.0.rate_of_fire);
        let aimed =
            state.winding || self.aim_body(tick, ids, entity, weapon, me.position, order.at)?;
        let shot = if aimed {
            tick_weapon(
                &self.combat,
                &mut state,
                now,
                false,
                me.position,
                rate,
                Self::bonus_conditions(tick.state(), ids, entity),
                tick.rng(),
            )
        } else {
            crate::combat::ShotEvent::Nothing
        };
        tick.state_mut().insert(ids.armed, entity, state)?;
        if shot == crate::combat::ShotEvent::WindUpStarted {
            self.emit_weapon_pre_attack_fx(tick, ids, weapon, me.position, entity);
        }
        if !shot.fired() {
            return Ok(());
        }
        self.note_horde_last_shot(tick, ids, entity);
        let landing = match template.resolve_shot(tick.rng(), false, false) {
            ShotOutcome::Struck => order.at,
            ShotOutcome::Missed([dx, dy]) => Vec2::new(order.at.x + dx, order.at.y + dy),
        };
        crate::diag::diag!(
            "shot: frame {} entity {} {} weapon {} slot {} at ground ({:.0}, {:.0}) range {:.0} lands {:.1} off",
            now.get(),
            entity.index(),
            self.kind_name(tick.state().get(ids.kind, entity).copied()),
            weapon.index(),
            WeaponSlot::from_index(order.slot.into()).map_or("?", WeaponSlot::name),
            order.at.x,
            order.at.y,
            (order.at.minus(me.position).length() - me.radius).max(0.0),
            landing.minus(order.at).length(),
        );
        let shot = Shot::at_ground(
            weapon,
            entity,
            self.take_launch(tick, entity, me, None),
            landing,
            self.ground_height(landing.x, landing.y),
        );
        self.release_shot(tick, ids, &shot, entity, landing, None, living, events)
    }
}
