use mint_movement::Vec2;
use mint_sage_modules::effects::{Name, Names};
use mint_sage_modules::{CastEdge, Effect, PowerSides};
use mint_sim::module::OwnerChange;
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::powers::PowerEnumEffect;
use crate::spell_state::{
    ArrowStormHits, CashBounty, CashBountyPower, PendingShot, ProductionBonuses, Suppressed,
};

pub(super) fn standing(
    rules: &RotwkRules,
    owner: PlayerId,
    other: Option<PlayerId>,
) -> (bool, bool, bool) {
    match other {
        Some(other) => (
            other == owner,
            rules.alliances.allied(owner, other),
            rules.alliances.hostile(owner, other),
        ),
        None => (false, false, false),
    }
}

pub(super) fn filter_of(tokens: &Names) -> ObjectFilter {
    let tokens: Vec<String> = tokens.iter().map(|name| name.to_string()).collect();
    ObjectFilter::parse(&tokens)
}

pub(super) fn sunbeam_grid(high: Vec2, spacing: f32) -> Vec<Vec2> {
    if !spacing.is_finite() || spacing <= 0.0 || !high.x.is_finite() || !high.y.is_finite() {
        return Vec::new();
    }
    let mut points = Vec::new();
    let mut y = spacing.trunc();
    while y < high.y - spacing {
        let mut x = spacing.trunc();
        while x < high.x - spacing {
            points.push(Vec2::new(x, y));
            x = (x + spacing).trunc();
        }
        y = (y + spacing).trunc();
    }
    points
}

impl RotwkRules {
    pub(super) fn give_timed_list(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        id: mint_sage_combat::ModifierId,
        until: Frame,
    ) -> Result<bool> {
        let Some(list) = self.combat.modifiers.get(id) else {
            return Ok(false);
        };
        let now = tick.frame();
        if !self.is_level_list(id) {
            let members = tick
                .state()
                .get(ids.horde, entity)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for member in members {
                if tick.state().contains(ids.body, member) {
                    self.give_timed_list(tick, ids, member, id, until)?;
                }
            }
        }
        if list.ignore_if_anti_category_active
            && tick
                .state()
                .get(ids.suppressed, entity)
                .is_some_and(|held| held.active(list.category.as_deref(), now))
        {
            return Ok(false);
        }
        let mut timed = tick
            .state()
            .get(ids.timed_modifiers, entity)
            .cloned()
            .unwrap_or_default();
        let mut dropped = false;
        if let (true, Some(category)) = (
            list.replace_in_category_if_longest,
            list.category.as_deref(),
        ) {
            let same = |held: mint_sage_combat::ModifierId| {
                held != id
                    && self
                        .combat
                        .modifiers
                        .get(held)
                        .and_then(|other| other.category.as_deref())
                        .is_some_and(|other| other.eq_ignore_ascii_case(category))
            };
            if timed
                .0
                .iter()
                .any(|(held, end)| same(*held) && *end >= until)
            {
                return Ok(false);
            }
            let before = timed.0.len();
            timed.0.retain(|(held, _)| !same(*held));
            dropped = timed.0.len() != before;
        }
        let new = timed.give(id, until);
        tick.state_mut()
            .insert(ids.timed_modifiers, entity, timed)?;
        if new {
            self.gain_power_list(tick, ids, entity, id)?;
        }
        if new || dropped {
            self.recompute_bonuses(tick, ids, entity)?;
        }
        self.list_given(tick, ids, entity, id, until, new)?;
        Ok(true)
    }

    pub(super) fn remove_timed_list(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        id: mint_sage_combat::ModifierId,
    ) -> Result<bool> {
        if !self.is_level_list(id) {
            let members = tick
                .state()
                .get(ids.horde, entity)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for member in members {
                self.remove_timed_list(tick, ids, member, id)?;
            }
        }
        let Some(mut timed) = tick.state().get(ids.timed_modifiers, entity).cloned() else {
            return Ok(false);
        };
        if !timed.remove(id) {
            return Ok(false);
        }
        if timed.0.is_empty() {
            tick.state_mut().remove(ids.timed_modifiers, entity);
        } else {
            tick.state_mut()
                .insert(ids.timed_modifiers, entity, timed)?;
        }
        self.recompute_bonuses(tick, ids, entity)?;
        Ok(true)
    }

    fn is_level_list(&self, id: mint_sage_combat::ModifierId) -> bool {
        self.combat
            .modifiers
            .get(id)
            .and_then(|list| list.category.as_deref())
            .is_some_and(|category| category.eq_ignore_ascii_case("LEVEL"))
    }

    pub(super) fn admits(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        owner: PlayerId,
        filter: &ObjectFilter,
        sides: PowerSides,
    ) -> bool {
        if world.contains(ids.doomed, entity)
            || world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
        {
            return false;
        }
        let other = world.get(ids.owner, entity).map(|o| o.0);
        let (same, allied, hostile) = standing(self, owner, other);
        let side_ok = match sides {
            PowerSides::Friends => same || allied,
            PowerSides::Enemies => hostile,
            PowerSides::All => true,
            PowerSides::FriendsAndEnemies => same || allied || hostile,
        };
        side_ok
            && filter.passes_relation(same, allied, hostile)
            && world.get(ids.kind, entity).is_some_and(|kind| {
                self.kinds
                    .get(*kind)
                    .is_some_and(|entry| filter.passes(&entry.record.kind_of, &entry.record.name))
            })
    }

    fn select_everywhere(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        filter: &ObjectFilter,
        sides: PowerSides,
    ) -> Vec<Entity> {
        world
            .store(ids.kind)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|entity| self.admits(world, ids, *entity, owner, filter, sides))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn pulse_aura(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        aura: &Effect,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let Effect::Aura {
            list,
            radius,
            affects,
            target_enemy,
            affect_good,
            affect_evil,
            allow_self,
            while_dead,
            anti,
            anti_fx,
            max_active_rank,
            contained_only,
            required_mounted,
            allow_while_attacking,
        } = aura
        else {
            return Ok(());
        };
        let world = tick.state();
        let dying = world.contains(ids.doomed, source)
            || world.get(ids.health, source).is_some_and(|h| h.0.is_dead());
        if dying && !while_dead {
            return Ok(());
        }
        let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(());
        };
        if self.holds_condition_in(world, source, "STONED")
            || (*required_mounted && !self.holds_condition_in(world, source, "MOUNTED"))
            || (!allow_while_attacking && Self::is_attacking(world, ids, source))
        {
            return Ok(());
        }
        let Some(id) = self.combat.modifiers.id(list) else {
            return Ok(());
        };
        let wall = self.has_kind_bit(world, ids, source, "WALK_ON_TOP_OF_WALL");
        let sides = if *affect_good || *affect_evil || (wall && !target_enemy) {
            PowerSides::All
        } else if *target_enemy {
            PowerSides::Enemies
        } else {
            PowerSides::Friends
        };
        let filter = filter_of(affects);
        let mut targets: Vec<Entity> = if *contained_only {
            world
                .get(ids.passengers, source)
                .map(|held| {
                    held.0
                        .iter()
                        .flat_map(|object| Self::object_and_members(world, ids, *object))
                        .filter(|entity| self.admits(world, ids, *entity, owner, &filter, sides))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            let radius = radius.unwrap_or_else(|| {
                if wall {
                    crate::construction::object_bounding_radius(
                        world,
                        &self.kinds,
                        (ids.kind, self.modules.as_ref().map(|wiring| wiring.state)),
                        source,
                    )
                } else {
                    0.0
                }
            });
            let mut near: Vec<u32> = Vec::new();
            grid.query_into(
                grid_cell(centre.x),
                grid_cell(centre.y),
                grid_radius_cells(radius),
                &mut near,
            );
            let reach = radius * radius;
            let mut found: Vec<Entity> = near
                .iter()
                .filter_map(|index| living.get(usize::try_from(*index).ok()?))
                .filter(|(foe, _)| {
                    let (dx, dy) = (foe.position.x - centre.x, foe.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
                .map(|(foe, _)| foe.entity)
                .filter(|entity| self.admits(world, ids, *entity, owner, &filter, sides))
                .collect();
            let hordes: Vec<Entity> = world
                .store(ids.horde)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default()
                .into_iter()
                .filter(|horde| {
                    world.get(ids.body, *horde).is_some_and(|body| {
                        let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                        dx * dx + dy * dy <= reach
                    })
                })
                .filter(|horde| self.admits(world, ids, *horde, owner, &filter, sides))
                .collect();
            if !self.is_level_list(id) {
                found.retain(|entity| {
                    !world
                        .get(ids.member, *entity)
                        .is_some_and(|member| hordes.contains(&member.horde))
                });
            }
            found.extend(hordes);
            found
        };
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        let now = tick.frame();
        let until = self.list_end(now, id);
        for entity in targets {
            if (entity == source && !allow_self)
                || !self.aura_reaches(tick.state(), ids, entity, *affect_good, *affect_evil)
                || (wall
                    && !tick
                        .state()
                        .get(ids.layer, entity)
                        .is_some_and(|layer| layer.is_wall_top()))
                || max_active_rank.is_some_and(|rank| {
                    rank > 0.0
                        && self
                            .experience_of(tick.state(), ids, entity)
                            .is_some_and(|(index, _)| (index as f32) + 1.0 > rank)
                })
            {
                continue;
            }
            if !anti.is_empty() {
                let mut suppressed = tick
                    .state()
                    .get(ids.suppressed, entity)
                    .cloned()
                    .unwrap_or_default();
                for category in anti.iter() {
                    suppressed.add(category, until);
                }
                tick.state_mut()
                    .insert(ids.suppressed, entity, suppressed)?;
                self.recompute_bonuses(tick, ids, entity)?;
                if let (Some(fx), Some(at)) = (
                    anti_fx,
                    tick.state().get(ids.body, entity).map(|b| b.0.position),
                ) {
                    let power = crate::fx::FxSource::Power;
                    self.emit_fx_object_named(tick, ids, fx, at, power, entity, None);
                }
            }
            self.give_timed_list(tick, ids, entity, id, until)?;
        }
        Ok(())
    }

    fn aura_reaches(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        affect_good: bool,
        affect_evil: bool,
    ) -> bool {
        if ["IGNORED_IN_GUI", "ARMY_OF_DEAD"]
            .iter()
            .any(|bit| self.has_kind_bit(world, ids, entity, bit))
            || self.holds_condition_in(world, entity, "STONED")
        {
            return false;
        }
        let evil = world
            .get(ids.owner, entity)
            .is_some_and(|owner| self.is_evil_owner(owner.0));
        (!affect_good || !evil) && (!affect_evil || evil)
    }

    pub(super) fn expire_spell_state(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.suppressed)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.0.iter().any(|(_, until)| *until <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in due {
            let Some(mut suppressed) = tick.state().get(ids.suppressed, entity).cloned() else {
                continue;
            };
            suppressed.expire(now);
            if suppressed.0.is_empty() {
                tick.state_mut().remove(ids.suppressed, entity);
            } else {
                tick.state_mut()
                    .insert(ids.suppressed, entity, suppressed)?;
            }
            self.recompute_bonuses(tick, ids, entity)?;
        }
        let ended: Vec<Entity> = tick
            .state()
            .store(ids.production_bonuses)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.0.iter().any(|(_, _, until)| *until <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in ended {
            let Some(mut bonuses) = tick.state().get(ids.production_bonuses, entity).cloned()
            else {
                continue;
            };
            if bonuses.expire(now) {
                tick.state_mut()
                    .insert(ids.production_bonuses, entity, bonuses)?;
            } else {
                tick.state_mut().remove(ids.production_bonuses, entity);
            }
        }
        Ok(())
    }

    pub(super) fn spaced_objects(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        name: &str,
        spacing: f32,
    ) -> Result<usize> {
        let Some(kind) = self.kinds.kind(name) else {
            diag!("power: {name} is not in this match's templates");
            return Ok(0);
        };
        let Some(owner) = self.neutral_player() else {
            diag!("power: {name} needs a neutral player, which this match has none of");
            return Ok(0);
        };
        let high = self.passability().playable_high;
        let mut made = 0usize;
        for at in sunbeam_grid(high, spacing) {
            if self
                .spawn_kind_at(tick, ids, kind, at, mint_movement::Facing::EAST, owner)?
                .is_some()
            {
                made = made.saturating_add(1);
            }
        }
        Ok(made)
    }

    pub(super) fn set_cash_bounty(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        fraction: f32,
    ) -> Result<()> {
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        if fraction.is_finite() {
            tick.state_mut()
                .insert(ids.cash_bounty, player, CashBounty(fraction))?;
            tick.state_mut()
                .insert(ids.cash_bounty_power, source, CashBountyPower(fraction))?;
            diag!(
                "power: player {} earns {:.0}% of a victim's bounty value per kill",
                owner.get(),
                fraction * 100.0
            );
        }
        Ok(())
    }

    pub(super) fn pause_special_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        disabled: bool,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return Ok(());
        };
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        crate::spell_state::pause_bounty(
            tick.state_mut(),
            (ids.cash_bounty, ids.cash_bounty_power),
            entity,
            player,
            disabled,
        )
    }

    pub(super) fn capture(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        affects: &Names,
    ) -> Result<usize> {
        let mut taken = 0usize;
        for entity in self.enemies_within(tick.state(), ids, owner, centre, radius, affects) {
            if self
                .modules
                .as_ref()
                .is_some_and(|wiring| wiring.carries_status(tick.state(), entity, "RAMPAGING"))
            {
                continue;
            }
            if self.change_owner(tick, ids, entity, owner)? {
                taken = taken.saturating_add(1);
            }
        }
        Ok(taken)
    }

    fn enemies_within(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        affects: &Names,
    ) -> Vec<Entity> {
        let filter = filter_of(affects);
        let reach = radius * radius;
        let mut targets: Vec<Entity> = self
            .select_everywhere(world, ids, owner, &filter, PowerSides::All)
            .into_iter()
            .filter(|entity| {
                !world
                    .get(ids.owner, *entity)
                    .is_some_and(|other| self.alliances.allied(owner, other.0))
            })
            .filter(|entity| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .collect();
        let members: Vec<Entity> = targets
            .iter()
            .filter_map(|entity| world.get(ids.horde, *entity))
            .flat_map(|horde| horde.members.iter().copied())
            .collect();
        targets.extend(members);
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        targets
    }

    fn dominate_may_target(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        entity: Entity,
    ) -> bool {
        entity != source
            && world
                .get(ids.owner, entity)
                .is_none_or(|other| other.0 != owner)
            && !world.contains(ids.construction, entity)
            && self.airborne_category(world, ids, entity).is_none()
    }

    fn dominate_targets_within(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        affects: &Names,
    ) -> Vec<Entity> {
        let filter = filter_of(affects);
        let reach = radius * radius;
        let mut targets: Vec<Entity> = self
            .select_everywhere(world, ids, owner, &filter, PowerSides::Enemies)
            .into_iter()
            .filter(|entity| self.dominate_may_target(world, ids, source, owner, *entity))
            .filter(|entity| !world.contains(ids.member, *entity))
            .filter(|entity| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .collect();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        targets
    }

    fn dominate_clicked_target(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        target: Entity,
        affects: &Names,
    ) -> Option<Entity> {
        if !self.dominate_may_target(world, ids, source, owner, target) {
            return None;
        }
        let filter = filter_of(affects);
        if !self.admits(world, ids, target, owner, &filter, PowerSides::All) {
            return None;
        }
        Some(
            world
                .get(ids.member, target)
                .map_or(target, |member| member.horde),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dominate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        affects: &Names,
        permanent: bool,
    ) -> Result<Vec<Entity>> {
        let reason = if permanent {
            OwnerChange::Capture
        } else {
            OwnerChange::Defect
        };
        let mut taken = Vec::new();
        for entity in
            self.dominate_targets_within(tick.state(), ids, source, owner, centre, radius, affects)
        {
            let was_defecting = tick.state().contains(ids.defect_revert, entity);
            if self.change_owner_for(tick, ids, entity, owner, reason)? {
                if !permanent {
                    self.set_defect_dominator(tick, ids, entity, source, was_defecting)?;
                }
                taken.push(entity);
            }
        }
        Ok(taken)
    }

    pub(super) fn dominate_clicked(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        target: Entity,
        affects: &Names,
        permanent: bool,
    ) -> Result<Vec<Entity>> {
        let Some(resolved) =
            self.dominate_clicked_target(tick.state(), ids, source, owner, target, affects)
        else {
            return Ok(Vec::new());
        };
        let reason = if permanent {
            OwnerChange::Capture
        } else {
            OwnerChange::Defect
        };
        let was_defecting = tick.state().contains(ids.defect_revert, resolved);
        if self.change_owner_for(tick, ids, resolved, owner, reason)? {
            if !permanent {
                self.set_defect_dominator(tick, ids, resolved, source, was_defecting)?;
            }
            Ok(vec![resolved])
        } else {
            Ok(Vec::new())
        }
    }

    pub(super) fn curse_may_reach(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        target: Entity,
    ) -> bool {
        let is_hero = world
            .get(ids.kind, target)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("HERO"));
        let (_, _, hostile) = standing(self, owner, world.get(ids.owner, target).map(|o| o.0));
        target != source && is_hero && hostile && !self.fails_power_scan_status(world, target)
    }

    pub(super) fn curse_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        target: Entity,
        share: f32,
    ) -> Result<usize> {
        if !share.is_finite() || share <= 0.0 {
            return Ok(0);
        }
        let now = tick.frame();
        let Some(mut book) = tick.state().get(ids.power_book, target).cloned() else {
            return Ok(0);
        };
        let mut cursed = 0usize;
        for instance in &mut book.0 {
            let reload = self
                .power_reload
                .get(usize::from(instance.power.get()))
                .and_then(|(reload, _)| *reload)
                .unwrap_or(LogicFrames::ZERO);
            let reload = self.measured_reload(tick.state(), ids, target, instance.power, reload);
            if reload.get() == 0 {
                continue;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let added = LogicFrames::from_frames((reload.get() as f32 * share) as u32);
            let ceiling = now.checked_add(reload).unwrap_or(Frame::FOREVER);
            let pushed = instance
                .available_on
                .max(now)
                .checked_add(added)
                .unwrap_or(Frame::FOREVER)
                .min(ceiling);
            if pushed > instance.available_on {
                instance.available_on = pushed;
                instance.reload = reload;
                cursed = cursed.saturating_add(1);
            }
        }
        if cursed > 0 {
            tick.state_mut().insert(ids.power_book, target, book)?;
        }
        Ok(cursed)
    }

    pub(super) fn power_enum_trigger(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
        edge: CastEdge,
    ) -> Result<()> {
        if edge != CastEdge::Trigger {
            return Ok(());
        }
        let effect = self
            .power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .map_or(PowerEnumEffect::InTheModule, |rule| rule.effect);
        match effect {
            PowerEnumEffect::InTheModule => Ok(()),
            PowerEnumEffect::HealBurst => self.heal_burst_power(tick, ids, source, power),
            PowerEnumEffect::Fear => self.screech_power(tick, ids, source, power),
        }
    }

    fn screech_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
    ) -> Result<()> {
        let range = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| {
                entry.record.declared_modules.iter().find_map(|module| {
                    module
                        .field("SpecialPowerTemplate")
                        .and_then(|field| field.args.first())
                        .is_some_and(|named| named.eq_ignore_ascii_case(power))
                        .then(|| module.field("EffectRange"))
                        .flatten()
                })
            })
            .and_then(|field| field.numeric)
            .filter(|range| range.is_finite())
            .unwrap_or(0.0);
        self.fire_object_event_at(
            tick,
            ids,
            source,
            "BeScary",
            None,
            Some(source),
            Some(mint_sage_modules::object_scripts::EventArg::Number(range)),
            0,
        )
    }

    fn heal_burst_power(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
    ) -> Result<()> {
        let Some(declaration) = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| {
                entry
                    .record
                    .declared_modules
                    .iter()
                    .find(|module| module.name.eq_ignore_ascii_case("AutoHealBehavior"))
            })
        else {
            diag!(
                "power: {power} at entity {} heals nothing - it declares no AutoHealBehavior",
                source.index()
            );
            return Ok(());
        };
        let numeric = |keyword: &str| {
            declaration
                .field(keyword)
                .and_then(|field| field.numeric)
                .filter(|value| value.is_finite())
        };
        let flag = |keyword: &str| {
            declaration
                .field(keyword)
                .and_then(|field| field.args.first())
                .is_some_and(|value| value.eq_ignore_ascii_case("Yes"))
        };
        if flag("HealOnlyIfNotInCombat") && Self::in_combat(tick.state(), ids, source) {
            return Ok(());
        }
        let bonus = tick
            .state()
            .get(ids.bonuses, source)
            .map_or(0.0, |bonuses| bonuses.0.auto_heal);
        let amount = (numeric("HealingAmount").unwrap_or(0.0) + bonus).trunc();
        let radius = numeric("Radius").unwrap_or(0.0);
        let kinds: Vec<String> = declaration
            .field("KindOf")
            .map(|field| field.args.clone())
            .unwrap_or_default();
        let skip_self = declaration
            .field("HealOnlyOthers")
            .and_then(|field| field.args.first())
            .is_some_and(|value| value.eq_ignore_ascii_case("Yes"));
        let fx = declaration
            .field("UnitHealPulseFX")
            .and_then(|field| field.args.first())
            .cloned();
        let healed = self.burst_heal(
            tick,
            ids,
            source,
            amount,
            radius,
            &kinds,
            skip_self,
            fx.as_deref(),
        )?;
        diag!(
            "power: {power} heals {healed} object(s) for {amount:.0} within {radius:.0} of \
             entity {}",
            source.index()
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn level_grant(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        experience: u32,
        affects: &Names,
    ) -> Result<Vec<Entity>> {
        let filter = filter_of(affects);
        let world = tick.state();
        let reach = radius * radius;
        let mut receivers: Vec<Entity> = self
            .select_everywhere(world, ids, owner, &filter, PowerSides::Friends)
            .into_iter()
            .filter(|entity| *entity != source)
            .filter(|entity| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .map(|entity| self.sink_of(world, ids, entity))
            .collect();
        receivers.sort_unstable_by_key(|entity| entity.index());
        receivers.dedup();
        let mut granted = Vec::with_capacity(receivers.len());
        for entity in receivers {
            if self.experience_of(tick.state(), ids, entity).is_none() {
                continue;
            }
            if tick.state().contains(ids.horde, entity) {
                if experience > 0 && self.grant_horde_pools(tick, ids, entity, experience)? {
                    granted.push(entity);
                }
                continue;
            }
            let scalar = self.level_scalar(tick.state(), ids, entity);
            let gain = crate::experience::scaled(experience, scalar);
            if gain > 0 {
                self.add_experience(tick, ids, entity, gain)?;
                granted.push(entity);
            }
        }
        Ok(granted)
    }

    pub(super) fn production_bonus(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        kinds: &Names,
        multiplier: f32,
        lasts: LogicFrames,
    ) -> Result<usize> {
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(0);
        };
        let until = tick.frame().checked_add(lasts).unwrap_or(Frame::FOREVER);
        let mut bonuses: ProductionBonuses = tick
            .state()
            .get(ids.production_bonuses, player)
            .cloned()
            .unwrap_or_default();
        let mut named = 0usize;
        for name in kinds.iter() {
            if let Some(kind) = self.kinds.kind(name) {
                bonuses.add(kind, multiplier, until);
                named = named.saturating_add(1);
            }
        }
        if named > 0 {
            tick.state_mut()
                .insert(ids.production_bonuses, player, bonuses)?;
        }
        Ok(named)
    }

    pub(super) fn category_suppressed(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        category: Option<&str>,
    ) -> bool {
        world
            .get(ids.suppressed, entity)
            .is_some_and(|held: &Suppressed| held.hides(category))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn arrow_storm_burst(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        weapon: &Name,
        centre: Vec2,
        radius: f32,
        shots_per_burst: u32,
        shots_per_target: u32,
        max_shots: u32,
        can_shoot_empty_ground: bool,
    ) -> Result<usize> {
        let Some(weapon_id) = self.combat.weapons.id(weapon) else {
            diag!("power: weapon {weapon} is not declared");
            return Ok(0);
        };
        let mut hits: ArrowStormHits = tick
            .state()
            .get(ids.arrow_storm_hits, source)
            .cloned()
            .unwrap_or_default();
        let remaining_total = max_shots.saturating_sub(hits.fired);
        if remaining_total == 0 {
            return Ok(0);
        }
        let mut shots: Vec<(Vec2, Option<Entity>)> = Vec::new();
        for _ in 0..shots_per_burst.min(remaining_total) {
            let target = {
                let world = tick.state();
                hits.next_target(shots_per_target, |entity| {
                    Self::arrow_storm_target_alive(world, ids, entity)
                })
            };
            match target {
                Some(entity) => {
                    let Some(at) = tick
                        .state()
                        .get(ids.body, entity)
                        .map(|body| body.0.position)
                    else {
                        continue;
                    };
                    shots.push((at, Some(entity)));
                }
                None if can_shoot_empty_ground => {
                    let dx = (tick.rng().next_f32() * 2.0 - 1.0) * radius;
                    let dy = (tick.rng().next_f32() * 2.0 - 1.0) * radius;
                    shots.push((Vec2::new(centre.x + dx, centre.y + dy), None));
                }
                None => break,
            }
            hits.fired = hits.fired.saturating_add(1);
        }
        for (at, victim) in &shots {
            self.queue_shot(
                tick,
                ids,
                PendingShot {
                    source,
                    weapon: weapon_id,
                    at: *at,
                    repeat: None,
                    alive_only: false,
                    reload_slot: None,
                    owner_if_gone: None,
                    victim: *victim,
                },
            )?;
        }
        let fired = shots.len();
        tick.state_mut()
            .insert(ids.arrow_storm_hits, source, hits)?;
        Ok(fired)
    }

    pub(super) fn sense_enemies(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        range: f32,
        filter: &ObjectFilter,
    ) -> Result<()> {
        const NEAR: &str = "SPECIAL_ENEMY_NEAR";
        let world = tick.state();
        let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|owner| owner.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(());
        };
        let reach = range * range;
        let near = world.store(ids.kind).is_some_and(|store| {
            store.entities().iter().any(|&entity| {
                entity != source
                    && self.admits(world, ids, entity, owner, filter, PowerSides::Enemies)
                    && world.get(ids.body, entity).is_some_and(|body| {
                        let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                        dx * dx + dy * dy <= reach
                    })
            })
        });
        let carriers = world
            .get(ids.passengers, source)
            .map_or_else(|| vec![source], |held| held.0.clone());
        let set = mint_sage_modules::effects::names(if near {
            vec![NEAR.to_owned()]
        } else {
            Vec::new()
        });
        let clear = mint_sage_modules::effects::names(if near {
            Vec::new()
        } else {
            vec![NEAR.to_owned()]
        });
        let effects = carriers
            .into_iter()
            .map(|carrier| {
                (
                    carrier,
                    Effect::ModelConditions {
                        add: Names::clone(&set),
                        remove: Names::clone(&clear),
                    },
                )
            })
            .collect();
        self.apply_effects(tick, ids, effects)
    }

    fn arrow_storm_target_alive(world: &World, ids: Ids, entity: Entity) -> bool {
        world.is_live(entity)
            && !world.contains(ids.doomed, entity)
            && !world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
    }

    pub(super) fn arrow_storm_shots(&self, world: &World, ids: Ids) -> Vec<(Entity, u32)> {
        world
            .store(ids.arrow_storm_hits)
            .map(|store| {
                store
                    .iter()
                    .map(|(caster, hits)| {
                        (
                            caster,
                            hits.shots_left(|entity| {
                                Self::arrow_storm_target_alive(world, ids, entity)
                            }),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn arrow_storm_prepare(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        source: Entity,
        (centre, radius): (Vec2, f32),
        (shots_per_target, max_shots): (u32, u32),
    ) -> Result<()> {
        let found = self.arrow_storm_scan(tick.state(), ids, owner, centre, radius);
        let list = crate::spell_state::arrow_storm_list(
            &found,
            usize::try_from(max_shots).unwrap_or(usize::MAX),
        );
        tick.state_mut().insert(
            ids.arrow_storm_hits,
            source,
            ArrowStormHits {
                list: Some(list),
                per_target: shots_per_target,
                max_shots,
                ..ArrowStormHits::default()
            },
        )?;
        Ok(())
    }

    fn arrow_storm_scan(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
    ) -> Vec<(Entity, bool)> {
        const SKIPPED: [&str; 6] = [
            "UNATTACKABLE",
            "IGNORED_IN_GUI",
            "INERT",
            "BASE_FOUNDATION",
            "MOVE_ONLY",
            "WALL_UPGRADE",
        ];
        let reach = radius * radius;
        let mut found: Vec<(Entity, bool)> = self
            .select_everywhere(
                world,
                ids,
                owner,
                &ObjectFilter::default(),
                PowerSides::Enemies,
            )
            .into_iter()
            .filter_map(|entity| {
                let body = world.get(ids.body, entity)?;
                let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                if dx * dx + dy * dy > reach {
                    return None;
                }
                let entry = world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))?;
                let has = |name: &str| entry.record.kind_of.iter().any(|flag| flag == name);
                (!SKIPPED.iter().any(|name| has(name))).then(|| (entity, has("MONSTER")))
            })
            .collect();
        found.sort_unstable_by_key(|(entity, _)| entity.index());
        found.dedup();
        found
    }

    pub(super) fn refresh_other_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        rule: &mint_sage_modules::ModifierRule,
    ) -> Result<usize> {
        let now = tick.frame();
        let world = tick.state();
        let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(0);
        };
        let recipients = self.modifier_recipients(tick, ids, source, owner, centre, rule);
        let mut total = 0usize;
        for entity in recipients {
            let count = self.reset_declared_powers(tick, ids, entity, now)?;
            total = total.saturating_add(count);
        }
        Ok(total)
    }

    pub(super) fn admitted_within(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        filter: &ObjectFilter,
    ) -> Vec<Entity> {
        let reach = radius * radius;
        let mut targets: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|entity| {
                        self.admits(world, ids, *entity, owner, filter, PowerSides::All)
                    })
                    .filter(|entity| {
                        world.get(ids.body, *entity).is_some_and(|body| {
                            let (dx, dy) =
                                (body.0.position.x - centre.x, body.0.position.y - centre.y);
                            dx * dx + dy * dy <= reach
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        targets
    }

    pub(super) fn hero_die(&self, tick: &mut Tick<'_>, ids: Ids, dying: Entity) -> Result<()> {
        let world = tick.state();
        let Some(entry) = world
            .get(ids.kind, dying)
            .and_then(|kind| self.kinds.get(*kind))
            .filter(|entry| entry.record.has_kind("HERO"))
        else {
            return Ok(());
        };
        let Some(owner) = world.get(ids.owner, dying).map(|owner| owner.0) else {
            return Ok(());
        };
        let Some(power) = entry
            .record
            .declared_modules
            .iter()
            .find(|module| module.name.eq_ignore_ascii_case("HeroDie"))
            .and_then(|module| module.field("SpecialPowerTemplate"))
            .and_then(|field| field.args.first().cloned())
        else {
            return Ok(());
        };
        let mut holders: Vec<Entity> = world
            .store(ids.owner)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.0 == owner)
                    .map(|(entity, _)| entity)
                    .filter(|entity| {
                        world
                            .get(ids.kind, *entity)
                            .and_then(|kind| self.kinds.get(*kind))
                            .is_some_and(|entry| entry.record.power(&power).is_some())
                    })
                    .collect()
            })
            .unwrap_or_default();
        holders.sort_unstable_by_key(|entity| entity.index());
        let now = tick.frame();
        for holder in holders {
            self.reset_power_ready(tick, ids, holder, &power, now)?;
        }
        Ok(())
    }

    fn reset_declared_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        now: Frame,
    ) -> Result<usize> {
        if tick.state().contains(ids.using_ability, entity) {
            return Ok(0);
        }
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(0);
        };
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(0);
        };
        let mut names: Vec<String> = entry
            .record
            .declared_modules
            .iter()
            .filter_map(|module| module.field("SpecialPowerTemplate"))
            .filter_map(|field| field.args.first().cloned())
            .collect();
        names.sort();
        names.dedup();
        names.retain(|name| {
            let shared = self.power_names.id(name).is_some_and(|id| {
                self.power_reload
                    .get(usize::from(id.get()))
                    .is_some_and(|(_, shared)| *shared)
            });
            !shared && !self.power_is_paused(tick.state(), ids, entity, name)
        });
        for name in &names {
            self.reset_power_ready(tick, ids, entity, name, now)?;
        }
        Ok(names.len())
    }

    pub(super) fn power_object_filter(&self, power_name: &str) -> Option<&ObjectFilter> {
        self.power_names
            .id(power_name)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .and_then(|rule| rule.object_filter.as_ref())
    }

    fn fails_power_scan_status(&self, world: &World, entity: Entity) -> bool {
        self.modules.as_ref().is_some_and(|wiring| {
            wiring.carries_status(world, entity, "UNDER_CONSTRUCTION")
                || wiring.carries_status(world, entity, "UNSELECTABLE")
        })
    }

    pub(super) fn store_objects_scan(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        power_name: &str,
        centre: Vec2,
        radius: f32,
    ) -> Vec<Entity> {
        let filter = self.power_object_filter(power_name);
        let reach = radius * radius;
        let mut targets: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|&entity| {
                        entity != source
                            && world.is_live(entity)
                            && !world.contains(ids.doomed, entity)
                            && !world.contains(ids.member, entity)
                            && !self.fails_power_scan_status(world, entity)
                    })
                    .filter(|&entity| {
                        world.get(ids.body, entity).is_some_and(|body| {
                            let d = body.0.position.minus(centre);
                            d.x * d.x + d.y * d.y <= reach
                        })
                    })
                    .filter(|&entity| {
                        filter.is_none_or(|filter| {
                            let other = world.get(ids.owner, entity).map(|o| o.0);
                            let (same, allied, hostile) = standing(self, owner, other);
                            filter.passes_relation(same, allied, hostile)
                                && world.get(ids.kind, entity).is_some_and(|kind| {
                                    self.kinds.get(*kind).is_some_and(|entry| {
                                        filter.passes(&entry.record.kind_of, &entry.record.name)
                                    })
                                })
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        targets
    }

    pub(super) fn teleport_gather_scan(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        power_name: &str,
        centre: Vec2,
        radius: f32,
    ) -> Vec<Entity> {
        self.store_objects_scan(world, ids, source, owner, power_name, centre, radius)
            .into_iter()
            .filter(|&entity| {
                world.get(ids.kind, entity).is_none_or(|kind| {
                    self.kinds.get(*kind).is_none_or(|entry| {
                        let kind_of = &entry.record.kind_of;
                        !(kind_of.iter().any(|k| k.eq_ignore_ascii_case("STRUCTURE"))
                            && kind_of.iter().any(|k| k.eq_ignore_ascii_case("MOVE_ONLY")))
                    })
                })
            })
            .collect()
    }

    pub(super) fn teleport_list(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        list: &[Entity],
        dest: Vec2,
        min: f32,
        max: f32,
    ) -> Result<Vec<Entity>> {
        let n = list.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let step = core::f32::consts::TAU / n as f32;
        let mut moved = Vec::new();
        for (i, &entity) in list.iter().enumerate() {
            if !tick.state().is_live(entity) {
                continue;
            }
            let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
                continue;
            };
            let start_angle = (i as f32) * step;
            let found = mint_sage_passability::find_position_around(
                &self.dynamic_view(tick, ids),
                self.passability.geometry,
                dest,
                (min, max, start_angle),
                mint_movement::MoverClass::new(mint_sage_passability::OBSTACLE),
                |_| true,
            );
            if let Some(spot) = found {
                if let Some(facing) = mint_movement::Facing::from_vector(spot.minus(dest)) {
                    body.0.facing = facing;
                }
                body.0.position = spot;
            } else {
                body.0.position = dest;
            }
            body.0.waypoint = 0;
            tick.state_mut().insert(ids.body, entity, body)?;
            tick.state_mut().remove(ids.path, entity);
            if tick.state().contains(ids.horde, entity) {
                self.snap_horde_members_to_slots(tick, ids, entity)?;
            }
            moved.push(entity);
        }
        Ok(moved)
    }

    pub(super) fn snap_horde_members_to_slots(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> Result<()> {
        self.reform_horde(tick, ids, horde_entity)?;
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let Some(facing) = tick.state().get(ids.body, horde_entity).map(|b| b.0.facing) else {
            return Ok(());
        };
        let horde_ids = Self::horde_ids(ids);
        for member in &horde.members {
            if !tick.state().is_live(*member) {
                continue;
            }
            let Some(pose) = crate::horde::member_slot_pose(tick.state(), horde_ids, *member)
            else {
                continue;
            };
            let Some(mut body) = tick.state().get(ids.body, *member).copied() else {
                continue;
            };
            body.0.position = pose;
            body.0.facing = facing;
            body.0.waypoint = 0;
            tick.state_mut().insert(ids.body, *member, body)?;
            tick.state_mut().remove(ids.path, *member);
        }
        Ok(())
    }

    pub(super) fn replace_in_area_scan(
        &self,
        world: &World,
        ids: Ids,
        source: Entity,
        owner: PlayerId,
        filter: &ObjectFilter,
        centre: Vec2,
        radius: f32,
    ) -> Vec<Entity> {
        let reach = radius * radius;
        let mut targets: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|&entity| {
                        entity != source
                            && world.is_live(entity)
                            && !world.contains(ids.doomed, entity)
                            && !world.contains(ids.contained, entity)
                    })
                    .filter(|&entity| {
                        world.get(ids.body, entity).is_some_and(|body| {
                            let d = body.0.position.minus(centre);
                            d.x * d.x + d.y * d.y <= reach
                        })
                    })
                    .filter(|&entity| {
                        let other = world.get(ids.owner, entity).map(|o| o.0);
                        let (same, allied, hostile) = standing(self, owner, other);
                        filter.passes_relation(same, allied, hostile)
                            && world.get(ids.kind, entity).is_some_and(|kind| {
                                self.kinds.get(*kind).is_some_and(|entry| {
                                    filter.passes(&entry.record.kind_of, &entry.record.name)
                                })
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sunbeam_grid_starts_a_spacing_in_and_clears_a_spacing_at_the_far_edge() {
        let points = sunbeam_grid(Vec2::new(1000.0, 700.0), 300.0);
        assert_eq!(
            points,
            vec![Vec2::new(300.0, 300.0), Vec2::new(600.0, 300.0)]
        );
        assert!(sunbeam_grid(Vec2::new(1000.0, 700.0), 0.0).is_empty());
        assert!(sunbeam_grid(Vec2::new(1000.0, 700.0), f32::NAN).is_empty());
    }

    #[test]
    fn standing_toward_a_genuinely_neutral_player_is_neither_allied_nor_hostile() {
        let map = mint_sage_map::HeightMapData {
            width: 16,
            height: 16,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![0; 16 * 16],
        };
        let passability = mint_sage_passability::Passability::build(&map, 5.0 / 128.0)
            .expect("a flat map builds");
        let rules = RotwkRules::new(passability, Vec::new())
            .with_teams(vec![None, None])
            .with_map_players(
                2,
                vec!["Civilians".to_owned()],
                vec![crate::alliance::MapRelations::default()],
            );
        let (player, neutral) = (PlayerId::new(0), PlayerId::new(2));
        assert_eq!(
            standing(&rules, player, Some(neutral)),
            (false, false, false)
        );
    }
}
