use mint_movement::Vec2;
use mint_sage_modules::effects::{Name, Names};
use mint_sage_modules::{CastEdge, PowerSides};
use mint_sim::module::OwnerChange;
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::powers::PowerEnumEffect;
use crate::spell_state::{ArrowStormHits, CashBounty, PendingShot, ProductionBonuses, Suppressed};

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
    ) -> Result<()> {
        let mut timed = tick
            .state()
            .get(ids.timed_modifiers, entity)
            .cloned()
            .unwrap_or_default();
        let new = timed.give(id, until);
        tick.state_mut()
            .insert(ids.timed_modifiers, entity, timed)?;
        if new {
            self.gain_power_list(tick, ids, entity, id)?;
            self.recompute_bonuses(tick, ids, entity)?;
        }
        Ok(())
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pulse_aura(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        list: &Name,
        radius: f32,
        affects: &Names,
        sides: PowerSides,
        allow_self: bool,
        lasts: LogicFrames,
        while_dead: bool,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
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
        let Some(id) = self.combat.modifiers.id(list) else {
            return Ok(());
        };
        let filter = filter_of(affects);
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(centre.x),
            grid_cell(centre.y),
            grid_radius_cells(radius),
            &mut near,
        );
        let reach = radius * radius;
        let mut targets: Vec<Entity> = near
            .iter()
            .filter_map(|index| living.get(usize::try_from(*index).ok()?))
            .filter(|(foe, _)| {
                let (dx, dy) = (foe.position.x - centre.x, foe.position.y - centre.y);
                dx * dx + dy * dy <= reach
            })
            .map(|(foe, _)| foe.entity)
            .filter(|entity| allow_self || *entity != source)
            .filter(|entity| self.admits(world, ids, *entity, owner, &filter, sides))
            .collect();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        let until = tick.frame().checked_add(lasts).unwrap_or(Frame::FOREVER);
        for entity in targets {
            self.give_timed_list(tick, ids, entity, id, until)?;
        }
        Ok(())
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

    pub(super) fn raise_cash_bounty(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        fraction: f32,
    ) -> Result<()> {
        let Some(player) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let held = tick
            .state()
            .get(ids.cash_bounty, player)
            .map_or(0.0, |bounty| bounty.0);
        if fraction.is_finite() && fraction > held {
            tick.state_mut()
                .insert(ids.cash_bounty, player, CashBounty(fraction))?;
            diag!(
                "power: player {} earns {:.0}% of a victim's bounty value per kill",
                owner.get(),
                fraction * 100.0
            );
        }
        Ok(())
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
            .select_everywhere(world, ids, owner, &filter, PowerSides::All)
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
        let Some(range) = tick
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
            .filter(|range| range.is_finite() && *range > 0.0)
        else {
            diag!(
                "power: {power} at entity {} reaches nobody - its module writes no EffectRange",
                source.index()
            );
            return Ok(());
        };
        let filter = ObjectFilter::parse(&["ANY".to_owned(), "ENEMIES".to_owned()]);
        self.emotion_pulse(
            tick,
            ids,
            source,
            super::emotions::Pulse {
                at: mint_sage_modules::PowerPlace::Source,
                radius: range,
                kind: mint_sage_ini::EmotionType::Fear,
                filter: &filter,
                lasts: LogicFrames::ZERO,
            },
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
        let amount = numeric("HealingAmount").unwrap_or(0.0);
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

    pub(super) fn level_grant(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        centre: Vec2,
        radius: f32,
        experience: u32,
        affects: &Names,
    ) -> Result<Vec<Entity>> {
        let filter = filter_of(affects);
        let world = tick.state();
        let reach = radius * radius;
        let mut targets: Vec<Entity> = self
            .select_everywhere(world, ids, owner, &filter, PowerSides::All)
            .into_iter()
            .filter(|entity| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .collect();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        for &entity in &targets {
            let scalar = self.level_scalar(tick.state(), ids, entity);
            let gain = crate::experience::scaled(experience, scalar);
            if gain > 0 {
                self.add_experience(tick, ids, entity, gain)?;
            }
        }
        Ok(targets)
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
        owner: PlayerId,
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
        let this_burst = shots_per_burst.min(remaining_total);

        let filter = ObjectFilter::default();
        let world = tick.state();
        let reach = radius * radius;
        let mut candidates: Vec<Entity> = self
            .select_everywhere(world, ids, owner, &filter, PowerSides::All)
            .into_iter()
            .filter(|&entity| entity != source)
            .filter(|&entity| {
                world.get(ids.body, entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .collect();
        candidates.sort_unstable_by_key(|entity| entity.index());
        candidates.dedup();

        let this_burst = usize::try_from(this_burst).unwrap_or(usize::MAX);
        let mut points: Vec<Vec2> = Vec::new();
        for &entity in &candidates {
            if points.len() >= this_burst {
                break;
            }
            if hits.count(entity.index()) < shots_per_target {
                if let Some(body) = world.get(ids.body, entity) {
                    points.push(body.0.position);
                    hits.record(entity.index());
                }
            }
        }
        if points.len() < this_burst {
            if can_shoot_empty_ground {
                while points.len() < this_burst {
                    points.push(centre);
                    hits.record_ground();
                }
            } else if !candidates.is_empty() {
                let needed = this_burst.saturating_sub(points.len());
                for entity in Self::spread_doubled_up_shots(&candidates, &mut hits, needed) {
                    if let Some(body) = world.get(ids.body, entity) {
                        points.push(body.0.position);
                    }
                }
            }
        }

        for point in &points {
            self.queue_shot(
                tick,
                ids,
                PendingShot {
                    source,
                    weapon: weapon_id,
                    at: *point,
                    repeat: None,
                    alive_only: false,
                    reload_slot: None,
                    owner_if_gone: None,
                },
            )?;
        }
        let fired = points.len();
        tick.state_mut()
            .insert(ids.arrow_storm_hits, source, hits)?;
        Ok(fired)
    }

    fn spread_doubled_up_shots(
        candidates: &[Entity],
        hits: &mut ArrowStormHits,
        needed: usize,
    ) -> Vec<Entity> {
        let mut picked = Vec::with_capacity(needed.min(candidates.len()));
        while picked.len() < needed {
            let Some(&entity) = candidates
                .iter()
                .min_by_key(|entity| (hits.count(entity.index()), entity.index()))
            else {
                break;
            };
            hits.record(entity.index());
            picked.push(entity);
        }
        picked
    }

    pub(super) fn refresh_other_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        except: &Name,
        recipients: Option<(f32, &Names)>,
    ) -> Result<usize> {
        let now = tick.frame();
        let mut total = self.reset_declared_powers(tick, ids, source, except, now)?;
        let Some((range, affects)) = recipients else {
            return Ok(total);
        };
        let world = tick.state();
        let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(total);
        };
        let mut targets =
            self.admitted_within(world, ids, owner, centre, range, &filter_of(affects));
        targets.retain(|entity| *entity != source);
        for entity in targets {
            let count = self.reset_declared_powers(tick, ids, entity, except, now)?;
            let name = tick
                .state()
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map_or("-", |entry| entry.record.name.as_str());
            diag!(
                "power: {except} refreshed {count} power(s) on entity {} ({name})",
                entity.index()
            );
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

    fn reset_declared_powers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        except: &Name,
        now: Frame,
    ) -> Result<usize> {
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
            .filter(|name| !name.eq_ignore_ascii_case(except))
            .collect();
        names.sort();
        names.dedup();
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

    fn candidates(n: u32) -> Vec<Entity> {
        let mut world = World::new();
        (0..n)
            .map(|_| world.spawn().expect("a fresh world always has room"))
            .collect()
    }

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
    fn doubled_up_shots_spread_to_whichever_target_has_fewer_hits() {
        let entities = candidates(2);
        let mut hits = ArrowStormHits::default();
        hits.record(entities[0].index());
        hits.record(entities[1].index());

        let picked = RotwkRules::spread_doubled_up_shots(&entities, &mut hits, 9);

        assert_eq!(picked.len(), 9);
        assert_eq!(hits.count(entities[0].index()), 6);
        assert_eq!(hits.count(entities[1].index()), 5);
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
