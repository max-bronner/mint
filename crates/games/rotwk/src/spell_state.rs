use mint_movement::Vec2;
use mint_sage_combat::{DamageTypeId, ModifierId, WeaponId, WeaponState};
use mint_sage_modules::effects::{Names, WeatherKind};
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, StateHasher};

use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingShot {
    pub(crate) source: Entity,
    pub(crate) weapon: WeaponId,
    pub(crate) at: Vec2,
    pub(crate) repeat: Option<u8>,
    pub(crate) alive_only: bool,
    pub(crate) reload_slot: Option<u32>,
    pub(crate) owner_if_gone: Option<PlayerId>,
    pub(crate) victim: Option<Entity>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PendingShots(pub(crate) Vec<PendingShot>);

impl Component for PendingShots {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for shot in &self.0 {
            hasher.write_u32(shot.source.index());
            shot.weapon.hash_into(hasher);
            hasher.write_f32(shot.at.x);
            hasher.write_f32(shot.at.y);
            hasher.write_u8(shot.repeat.map_or(u8::MAX, |n| n));
            hasher.write_bool(shot.alive_only);
            hasher.write_u32(shot.reload_slot.map_or(u32::MAX, |n| n));
            hasher.write_u32(shot.owner_if_gone.map_or(u32::MAX, |p| u32::from(p.get())));
            hasher.write_u32(shot.victim.map_or(u32::MAX, |v| v.index()));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DelayedHit {
    pub(crate) source: Entity,
    pub(crate) weapon: WeaponId,
    pub(crate) victim: Option<Entity>,
    pub(crate) at: Vec2,
    pub(crate) height: f32,
    pub(crate) due: Frame,
    pub(crate) nugget: Option<u16>,
    pub(crate) owner_if_gone: Option<PlayerId>,
    pub(crate) blow: Option<crate::combat::DamageEvent>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DelayedHits(pub(crate) Vec<DelayedHit>);

impl Component for DelayedHits {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for hit in &self.0 {
            hasher.write_u32(hit.source.index());
            hit.weapon.hash_into(hasher);
            hasher.write_u32(hit.victim.map_or(u32::MAX, Entity::index));
            hasher.write_f32(hit.at.x);
            hasher.write_f32(hit.at.y);
            hasher.write_f32(hit.height);
            hit.due.hash_into(hasher);
            hasher.write_u32(hit.nugget.map_or(u32::MAX, u32::from));
            hasher.write_u32(hit.owner_if_gone.map_or(u32::MAX, |p| u32::from(p.get())));
            hasher.write_u32(hit.blow.map_or(u32::MAX, |blow| blow.victim.index()));
            hasher.write_f32(hit.blow.map_or(0.0, |blow| blow.amount));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingImpact {
    pub(crate) attacker: Entity,
    pub(crate) victim: Entity,
    pub(crate) weapon: WeaponId,
    pub(crate) nugget: u16,
    pub(crate) centre: Vec2,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PendingImpacts(pub(crate) Vec<PendingImpact>);

impl Component for PendingImpacts {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for impact in &self.0 {
            hasher.write_u32(impact.attacker.index());
            hasher.write_u32(impact.victim.index());
            impact.weapon.hash_into(hasher);
            hasher.write_u16(impact.nugget);
            hasher.write_f32(impact.centre.x);
            hasher.write_f32(impact.centre.y);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct WeaponClocks(pub(crate) Vec<(u8, WeaponState)>);

impl WeaponClocks {
    pub(crate) fn clock(
        &mut self,
        nugget: u8,
        template: &mint_sage_combat::WeaponTemplate,
    ) -> Option<&mut WeaponState> {
        if !self.0.iter().any(|(held, _)| *held == nugget) {
            self.0.push((nugget, WeaponState::new(template)));
        }
        self.0
            .iter_mut()
            .find(|(held, _)| *held == nugget)
            .map(|(_, state)| state)
    }
}

impl Component for WeaponClocks {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (nugget, state) in &self.0 {
            hasher.write_u8(*nugget);
            state.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct AbilityWeaponClocks(pub(crate) Vec<(u32, WeaponState)>);

impl AbilityWeaponClocks {
    pub(crate) fn clock(
        &mut self,
        slot: u32,
        template: &mint_sage_combat::WeaponTemplate,
    ) -> Option<&mut WeaponState> {
        if !self.0.iter().any(|(held, _)| *held == slot) {
            self.0.push((slot, WeaponState::new(template)));
        }
        self.0
            .iter_mut()
            .find(|(held, _)| *held == slot)
            .map(|(_, state)| state)
    }
}

impl Component for AbilityWeaponClocks {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (slot, state) in &self.0 {
            hasher.write_u32(*slot);
            state.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct StoredObjectLists(pub(crate) Vec<(u32, Vec<Entity>)>);

impl StoredObjectLists {
    pub(crate) fn store(&mut self, slot: u32, list: Vec<Entity>) {
        match self.0.iter_mut().find(|(held, _)| *held == slot) {
            Some((_, held)) => *held = list,
            None => self.0.push((slot, list)),
        }
    }

    pub(crate) fn first_non_empty(&mut self) -> Option<&mut Vec<Entity>> {
        self.0.sort_unstable_by_key(|(slot, _)| *slot);
        self.0
            .iter_mut()
            .find(|(_, list)| !list.is_empty())
            .map(|(_, list)| list)
    }
}

impl Component for StoredObjectLists {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (slot, list) in &self.0 {
            hasher.write_u32(*slot);
            hasher.write_usize(list.len());
            for entity in list {
                hasher.write_u32(entity.index());
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Dot {
    pub(crate) attacker: Entity,
    pub(crate) amount: f32,
    pub(crate) damage_type: DamageTypeId,
    pub(crate) interval: LogicFrames,
    pub(crate) next: Frame,
    pub(crate) until: Frame,
    pub(crate) death: crate::combat::DeathNaming,
    pub(crate) sub_type: Option<DamageTypeId>,
    pub(crate) accepts_add: bool,
    pub(crate) from_nugget: bool,
}

impl Dot {
    pub(crate) fn pulse_event(&self, victim: Entity) -> crate::combat::DamageEvent {
        crate::combat::DamageEvent {
            victim,
            attacker: self.attacker,
            amount: self.amount,
            damage_type: self.damage_type,
            scalar: 1.0,
            fx_type: None,
            dealt: 0.0,
            death: self.death,
            sub_type: self.sub_type,
            flanking_bonus: 0.0,
            weapon: None,
            launched: None,
            nugget: 0,
            accepts_add: self.accepts_add,
            bonuses_applied: self.from_nugget,
            force_kill: false,
            drain_life: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ArrowStormHits {
    pub(crate) fired: u32,
    pub(crate) per_target: Vec<(u32, u32)>,
}

impl ArrowStormHits {
    pub(crate) fn count(&self, target: u32) -> u32 {
        self.per_target
            .iter()
            .find(|(index, _)| *index == target)
            .map_or(0, |(_, count)| *count)
    }

    pub(crate) fn record(&mut self, target: u32) {
        self.fired = self.fired.saturating_add(1);
        match self
            .per_target
            .iter_mut()
            .find(|(index, _)| *index == target)
        {
            Some((_, count)) => *count = count.saturating_add(1),
            None => self.per_target.push((target, 1)),
        }
    }

    pub(crate) fn record_ground(&mut self) {
        self.fired = self.fired.saturating_add(1);
    }
}

impl Component for ArrowStormHits {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.fired);
        hasher.write_usize(self.per_target.len());
        for (index, count) in &self.per_target {
            hasher.write_u32(*index);
            hasher.write_u32(*count);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DamageOverTime(pub(crate) Vec<Dot>);

impl DamageOverTime {
    pub(crate) fn hit(&mut self, now: Frame, hit: &crate::combat::DotHit) {
        let interval = if hit.interval.is_zero() {
            LogicFrames::ONE
        } else {
            hit.interval
        };
        let until = now.checked_add(hit.duration).unwrap_or(Frame::FOREVER);
        if hit.from_nugget {
            let fresh = Dot {
                attacker: hit.attacker,
                amount: hit.amount,
                damage_type: hit.damage_type,
                interval,
                next: now.checked_add(interval).unwrap_or(Frame::FOREVER),
                until,
                death: hit.death,
                sub_type: hit.sub_type,
                accepts_add: hit.accepts_add,
                from_nugget: true,
            };
            match self.0.iter_mut().find(|dot| dot.from_nugget) {
                Some(old) => {
                    let remaining = |end: Frame| end.get().saturating_sub(now.get()) as f32;
                    if remaining(until) * hit.amount > remaining(old.until) * old.amount {
                        *old = fresh;
                    }
                }
                None => self.0.push(fresh),
            }
            return;
        }
        match self
            .0
            .iter_mut()
            .find(|dot| !dot.from_nugget && dot.damage_type == hit.damage_type)
        {
            Some(dot) => {
                dot.attacker = hit.attacker;
                dot.amount = hit.amount;
                dot.until = dot.until.max(until);
                dot.death = hit.death;
                dot.sub_type = hit.sub_type;
                dot.accepts_add = hit.accepts_add;
            }
            None => self.0.push(Dot {
                attacker: hit.attacker,
                amount: hit.amount,
                damage_type: hit.damage_type,
                interval,
                next: now.checked_add(interval).unwrap_or(Frame::FOREVER),
                until,
                death: hit.death,
                sub_type: hit.sub_type,
                accepts_add: hit.accepts_add,
                from_nugget: false,
            }),
        }
    }

    pub(crate) fn pulse(&mut self, now: Frame, mut out: impl FnMut(Dot)) -> bool {
        for dot in &mut self.0 {
            if dot.next <= now && dot.next <= dot.until {
                out(*dot);
                dot.next = now.checked_add(dot.interval).unwrap_or(Frame::FOREVER);
            }
        }
        self.0.retain(|dot| dot.next <= dot.until);
        !self.0.is_empty()
    }
}

impl Component for DamageOverTime {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for dot in &self.0 {
            hasher.write_u32(dot.attacker.index());
            hasher.write_f32(dot.amount);
            dot.damage_type.hash_into(hasher);
            dot.interval.hash_into(hasher);
            dot.next.hash_into(hasher);
            dot.until.hash_into(hasher);
            dot.death.hash_into(hasher);
            match dot.sub_type {
                Some(sub) => {
                    hasher.write_u8(1);
                    sub.hash_into(hasher);
                }
                None => hasher.write_u8(0),
            }
            hasher.write_bool(dot.accepts_add);
            hasher.write_bool(dot.from_nugget);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Suppressed(pub(crate) Vec<(String, Frame)>);

impl Suppressed {
    pub(crate) fn add(&mut self, category: &str, until: Frame) {
        let key = category.to_ascii_uppercase();
        match self.0.iter_mut().find(|(held, _)| *held == key) {
            Some((_, end)) => *end = until,
            None => self.0.push((key, until)),
        }
    }

    pub(crate) fn hides(&self, category: Option<&str>) -> bool {
        category.is_some_and(|category| {
            self.0
                .iter()
                .any(|(held, _)| held.eq_ignore_ascii_case(category))
        })
    }

    pub(crate) fn expire(&mut self, now: Frame) -> bool {
        let before = self.0.len();
        self.0.retain(|(_, until)| *until > now);
        self.0.len() != before
    }

    pub(crate) fn remove(&mut self, category: &str) -> bool {
        let before = self.0.len();
        self.0
            .retain(|(held, _)| !held.eq_ignore_ascii_case(category));
        self.0.len() != before
    }
}

impl Component for Suppressed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (category, until) in &self.0 {
            hasher.write_usize(category.len());
            hasher.write_bytes(category.as_bytes());
            until.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct CashBounty(pub(crate) f32);

impl Component for CashBounty {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct CashBountyPower(pub(crate) f32);

impl Component for CashBountyPower {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

pub(crate) fn bounty_fraction(own: f32, players: f32) -> f32 {
    if own != 0.0 {
        own
    } else {
        players
    }
}

pub(crate) fn pause_bounty(
    world: &mut mint_sim::World,
    ids: (
        mint_sim::ComponentId<CashBounty>,
        mint_sim::ComponentId<CashBountyPower>,
    ),
    source: Entity,
    player: Entity,
    disabled: bool,
) -> mint_sim::Result<()> {
    let Some(CashBountyPower(fraction)) = world.get(ids.1, source).copied() else {
        return Ok(());
    };
    let bounty = if disabled { 0.0 } else { fraction };
    world.insert(ids.0, player, CashBounty(bounty))?;
    Ok(())
}

pub(crate) fn restore_bounty(
    world: &mut mint_sim::World,
    id: mint_sim::ComponentId<CashBounty>,
    player: Entity,
    bounty: f32,
) -> mint_sim::Result<()> {
    if bounty.is_finite() && bounty > 0.0 {
        world.insert(id, player, CashBounty(bounty))?;
    } else {
        world.remove(id, player);
    }
    Ok(())
}

pub(crate) fn bounty_for(value: f32, fraction: f32) -> i64 {
    let product = value * fraction;
    if !product.is_finite() || product <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = product.min(1.0e12).ceil() as i64;
    whole
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ProductionBonuses(pub(crate) Vec<(Kind, f32, Frame)>);

impl ProductionBonuses {
    pub(crate) fn add(&mut self, kind: Kind, multiplier: f32, until: Frame) {
        match self.0.iter_mut().find(|(held, ..)| *held == kind) {
            Some((_, held, end)) => {
                *held = held.max(multiplier);
                *end = (*end).max(until);
            }
            None => self.0.push((kind, multiplier, until)),
        }
    }

    pub(crate) fn frames_for(&self, kind: Kind, now: Frame) -> u32 {
        self.0
            .iter()
            .find(|(held, _, until)| *held == kind && now < *until)
            .map_or(1, |(_, multiplier, _)| {
                if multiplier.is_finite() && *multiplier >= 1.0 {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let whole = multiplier.min(1000.0) as u32;
                    whole
                } else {
                    1
                }
            })
    }

    pub(crate) fn expire(&mut self, now: Frame) -> bool {
        self.0.retain(|(_, _, until)| *until > now);
        !self.0.is_empty()
    }
}

impl Component for ProductionBonuses {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (kind, multiplier, until) in &self.0 {
            hasher.write_u16(kind.0);
            hasher.write_f32(*multiplier);
            until.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LastHurt(pub(crate) Option<Frame>);

impl LastHurt {
    pub(crate) fn quiet_for(self, window: LogicFrames, now: Frame) -> bool {
        self.0
            .is_none_or(|hurt| hurt.checked_add(window).is_some_and(|end| now >= end))
    }
}

impl Component for LastHurt {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.0 {
            Some(frame) => {
                hasher.write_bool(true);
                frame.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SoleHealer {
    pub(crate) by: Entity,
    pub(crate) until: Frame,
}

impl SoleHealer {
    pub(crate) fn admits(held: Option<Self>, healer: Entity, now: Frame) -> bool {
        held.is_none_or(|held| now > held.until || held.by == healer)
    }
}

impl Component for SoleHealer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.by.index());
        self.until.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct HealedAt(pub(crate) Option<Frame>);

impl HealedAt {
    pub(crate) fn quiet_for(self, window: LogicFrames, now: Frame) -> bool {
        self.0
            .is_none_or(|healed| healed.checked_add(window).is_some_and(|end| now >= end))
    }
}

impl Component for HealedAt {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.0 {
            Some(frame) => {
                hasher.write_bool(true);
                frame.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointBonus(pub(crate) i32);

impl Component for PointBonus {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct SpellRecharge {
    pub(crate) count: i32,
    pub(crate) modifier: f32,
}

impl SpellRecharge {
    pub(crate) fn moved(self, delta: i32, percentages: &[f32]) -> Self {
        let count = self.count.saturating_add(delta);
        let last = percentages.len().saturating_sub(1);
        let index = usize::try_from(count.saturating_sub(1))
            .unwrap_or(0)
            .min(last);
        Self {
            count,
            modifier: percentages.get(index).copied().unwrap_or(self.modifier),
        }
    }
}

impl Component for SpellRecharge {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.count);
        hasher.write_f32(self.modifier);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointRule {
    pub(crate) start: i32,
    pub(crate) cap: Option<i32>,
    pub(crate) factor: u32,
    pub(crate) overridden: bool,
}

impl PointRule {
    pub(crate) fn limit(self, bonus: i32) -> i32 {
        let raw = i64::from(self.start).saturating_add(i64::from(bonus));
        let factor = i64::from(self.factor);
        let limit = match self.cap {
            Some(cap) => raw.min(i64::from(cap).saturating_mul(factor) / 100),
            None => raw.saturating_mul(factor) / 100,
        };
        i32::try_from(limit.max(0)).unwrap_or(i32::MAX)
    }

    pub(crate) const fn overridden(base: i32, cap: i32) -> Self {
        Self {
            start: base,
            cap: Some(cap),
            factor: 100,
            overridden: true,
        }
    }
}

impl Component for PointRule {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.start);
        hasher.write_i32(self.cap.unwrap_or(-1));
        hasher.write_u32(self.factor);
        if self.overridden {
            hasher.write_u8(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Weather {
    pub(crate) kind: WeatherKind,
    pub(crate) timer: LogicFrames,
}

impl Component for Weather {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(match self.kind {
            WeatherKind::None => 0,
            WeatherKind::Cloudy => 1,
            WeatherKind::Rainy => 2,
            WeatherKind::CloudyRainy => 3,
            WeatherKind::Sunny => 4,
        });
        self.timer.hash_into(hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct WeatherEffect {
    pub(crate) list: Option<ModifierId>,
    pub(crate) anti: Names,
    pub(crate) affects: Names,
    pub(crate) affect_good: bool,
    pub(crate) affect_evil: bool,
    pub(crate) caster: PlayerId,
    pub(crate) timer: LogicFrames,
    pub(crate) recipients: Vec<Entity>,
}

impl Component for WeatherEffect {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.list.map_or(u16::MAX, ModifierId::index));
        hasher.write_usize(self.anti.len());
        for category in self.anti.iter() {
            hasher.write_usize(category.len());
            hasher.write_bytes(category.as_bytes());
        }
        hasher.write_usize(self.affects.len());
        for token in self.affects.iter() {
            hasher.write_usize(token.len());
            hasher.write_bytes(token.as_bytes());
        }
        hasher.write_u8(u8::from(self.affect_good));
        hasher.write_u8(u8::from(self.affect_evil));
        self.caster.hash_into(hasher);
        self.timer.hash_into(hasher);
        hasher.write_usize(self.recipients.len());
        for entity in &self.recipients {
            entity.hash_into(hasher);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sage_combat::DamageTypes;

    #[test]
    fn a_non_zero_killer_bounty_wins_over_the_players_otherwise_the_players_applies() {
        assert_eq!(bounty_fraction(0.5, 1.0), 0.5);
        assert_eq!(bounty_fraction(0.0, 1.0), 1.0);
    }

    #[test]
    fn a_disabled_bounty_power_holds_the_bounty_at_zero_until_it_is_enabled() {
        let mut world = mint_sim::World::new();
        let bounty = world.register::<CashBounty>();
        let power = world.register::<CashBountyPower>();
        let (source, player) = (world.spawn().unwrap(), world.spawn().unwrap());
        let other = world.spawn().unwrap();
        world.insert(power, source, CashBountyPower(0.5)).unwrap();
        pause_bounty(&mut world, (bounty, power), source, player, true).unwrap();
        assert_eq!(world.get(bounty, player), Some(&CashBounty(0.0)));
        pause_bounty(&mut world, (bounty, power), source, player, false).unwrap();
        assert_eq!(world.get(bounty, player), Some(&CashBounty(0.5)));
        pause_bounty(&mut world, (bounty, power), other, player, true).unwrap();
        assert_eq!(
            world.get(bounty, player),
            Some(&CashBounty(0.5)),
            "no power, no change"
        );
    }

    #[test]
    fn restoring_a_zero_bounty_clears_the_one_the_player_held() {
        let mut world = mint_sim::World::new();
        let id = world.register::<CashBounty>();
        let player = world.spawn().unwrap();
        world.insert(id, player, CashBounty(0.7)).unwrap();
        restore_bounty(&mut world, id, player, 0.0).unwrap();
        assert!(world.get(id, player).is_none());
        restore_bounty(&mut world, id, player, 0.4).unwrap();
        assert_eq!(world.get(id, player), Some(&CashBounty(0.4)));
    }

    fn entity(index: u32) -> Entity {
        let mut world = mint_sim::World::new();
        let mut last = world.spawn().unwrap();
        for _ in 0..index {
            last = world.spawn().unwrap();
        }
        last
    }

    fn nugget_hit(
        attacker: Entity,
        amount: f32,
        damage_type: DamageTypeId,
        interval: u32,
        duration: u32,
    ) -> crate::combat::DotHit {
        crate::combat::DotHit {
            from_nugget: true,
            ..dot_hit(attacker, amount, damage_type, interval, duration)
        }
    }

    fn dot_hit(
        attacker: Entity,
        amount: f32,
        damage_type: DamageTypeId,
        interval: u32,
        duration: u32,
    ) -> crate::combat::DotHit {
        crate::combat::DotHit {
            victim: attacker,
            attacker,
            amount,
            damage_type,
            interval: LogicFrames::from_frames(interval),
            duration: LogicFrames::from_frames(duration),
            death: crate::combat::DeathNaming::Normal,
            sub_type: None,
            accepts_add: true,
            from_nugget: false,
        }
    }

    #[test]
    fn a_dot_pulses_every_interval_until_its_duration_ends() {
        let mut types = DamageTypes::new();
        let poison = types.intern("POISON").unwrap();
        let mut dots = DamageOverTime::default();
        dots.hit(Frame::new(10), &dot_hit(entity(1), 5.0, poison, 15, 90));
        let mut pulses = Vec::new();
        for frame in 10..120u64 {
            let left = dots.pulse(Frame::new(frame), |dot| pulses.push((frame, dot.amount)));
            if !left {
                break;
            }
        }
        let frames: Vec<u64> = pulses.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![25, 40, 55, 70, 85, 100]);
        assert!(dots.0.is_empty());
    }

    #[test]
    fn a_fresh_hit_refreshes_rather_than_stacks() {
        let mut types = DamageTypes::new();
        let poison = types.intern("POISON").unwrap();
        let flame = types.intern("FLAME").unwrap();
        let mut dots = DamageOverTime::default();
        dots.hit(Frame::new(0), &dot_hit(entity(1), 5.0, poison, 10, 30));
        dots.hit(Frame::new(5), &dot_hit(entity(2), 8.0, poison, 10, 60));
        dots.hit(Frame::new(5), &dot_hit(entity(2), 1.0, flame, 10, 0));
        assert_eq!(dots.0.len(), 2);
        assert_eq!(dots.0[0].amount, 8.0);
        assert_eq!(dots.0[0].next, Frame::new(10), "the clock kept its phase");
        assert_eq!(dots.0[0].until, Frame::new(65));
    }

    #[test]
    fn a_dot_nugget_is_replaced_only_by_one_with_more_left_to_deal() {
        let mut types = DamageTypes::new();
        let poison = types.intern("POISON").unwrap();
        let flame = types.intern("FLAME").unwrap();
        let mut dots = DamageOverTime::default();
        dots.hit(Frame::new(0), &nugget_hit(entity(1), 10.0, poison, 30, 300));
        dots.hit(Frame::new(100), &nugget_hit(entity(2), 4.0, flame, 10, 300));
        assert_eq!(dots.0.len(), 1);
        assert_eq!(dots.0[0].amount, 10.0, "200 left x 10 outweighs 300 x 4");
        dots.hit(
            Frame::new(100),
            &nugget_hit(entity(3), 10.0, flame, 20, 250),
        );
        assert_eq!(dots.0.len(), 1);
        assert_eq!(dots.0[0].attacker, entity(3), "250 x 10 beats 200 x 10");
        assert_eq!(dots.0[0].damage_type, flame);
        assert_eq!(dots.0[0].next, Frame::new(120));
        assert_eq!(dots.0[0].until, Frame::new(350));
    }

    #[test]
    fn a_burning_pulse_sits_beside_a_dot_nugget() {
        let mut types = DamageTypes::new();
        let poison = types.intern("POISON").unwrap();
        let flame = types.intern("FLAME").unwrap();
        let mut dots = DamageOverTime::default();
        dots.hit(Frame::new(0), &nugget_hit(entity(1), 10.0, poison, 30, 300));
        dots.hit(Frame::new(0), &dot_hit(entity(1), 1.0, flame, 10, 0));
        assert_eq!(dots.0.len(), 2);
    }

    #[test]
    fn a_nugget_dot_pulse_is_not_scaled_again_by_the_attackers_bonuses() {
        let mut types = DamageTypes::new();
        let poison = types.intern("POISON").unwrap();
        let mut dots = DamageOverTime::default();
        dots.hit(Frame::new(0), &nugget_hit(entity(1), 10.0, poison, 30, 300));
        dots.hit(Frame::new(0), &dot_hit(entity(1), 1.0, poison, 10, 0));
        assert!(dots.0[0].pulse_event(entity(2)).bonuses_applied);
        assert!(!dots.0[1].pulse_event(entity(2)).bonuses_applied);
    }

    #[test]
    fn a_suppression_hides_its_category_until_it_expires() {
        let mut suppressed = Suppressed::default();
        suppressed.add("leadership", Frame::new(100));
        assert!(suppressed.hides(Some("LEADERSHIP")));
        assert!(!suppressed.hides(Some("SPELL")));
        assert!(!suppressed.hides(None));
        assert!(!suppressed.expire(Frame::new(99)));
        assert!(suppressed.expire(Frame::new(100)));
        assert!(!suppressed.hides(Some("LEADERSHIP")));
    }

    #[test]
    fn a_later_stamp_overwrites_rather_than_extends() {
        let mut suppressed = Suppressed::default();
        suppressed.add("LEADERSHIP", Frame::new(100));
        suppressed.add("leadership", Frame::new(50));
        assert!(suppressed.expire(Frame::new(50)), "the shorter stamp won");
        assert!(!suppressed.hides(Some("LEADERSHIP")));
    }

    #[test]
    fn a_bounty_rounds_up_and_refuses_nonsense() {
        assert_eq!(bounty_for(4.0, 1.0), 4);
        assert_eq!(bounty_for(4.0, 0.3), 2);
        assert_eq!(bounty_for(4.0, 0.0), 0);
        assert_eq!(bounty_for(f32::NAN, 1.0), 0);
        assert_eq!(bounty_for(4.0, -1.0), 0);
    }

    #[test]
    fn a_production_bonus_speeds_its_kind_until_it_ends() {
        let mut bonuses = ProductionBonuses::default();
        bonuses.add(Kind(3), 20.0, Frame::new(180));
        assert_eq!(bonuses.frames_for(Kind(3), Frame::new(179)), 20);
        assert_eq!(bonuses.frames_for(Kind(3), Frame::new(180)), 1);
        assert_eq!(bonuses.frames_for(Kind(4), Frame::new(0)), 1);
        bonuses.add(Kind(3), 2.0, Frame::new(300));
        assert_eq!(
            bonuses.frames_for(Kind(3), Frame::new(200)),
            20,
            "the larger kept"
        );
        assert!(bonuses.expire(Frame::new(299)));
        assert!(!bonuses.expire(Frame::new(300)));
    }

    #[test]
    fn a_weapon_clock_is_created_ready_and_kept_per_nugget() {
        let template = mint_sage_combat::WeaponTemplate {
            delay_between_shots: mint_sage_combat::FrameRange::fixed(LogicFrames::from_frames(30)),
            ..mint_sage_combat::WeaponTemplate::default()
        };
        let mut clocks = WeaponClocks::default();
        let clock = clocks.clock(2, &template).unwrap();
        assert_eq!(
            clock.status(Frame::new(0)),
            mint_sage_combat::WeaponStatus::ReadyToFire
        );
        clock.fire(&template, Frame::new(0), &mut mint_sim::Pcg32::new(1));
        assert_ne!(
            clocks.clock(2, &template).unwrap().status(Frame::new(10)),
            mint_sage_combat::WeaponStatus::ReadyToFire
        );
        assert_eq!(clocks.0.len(), 1);
        clocks.clock(0, &template);
        assert_eq!(clocks.0.len(), 2);
    }

    #[test]
    fn a_self_healer_is_quiet_once_its_window_has_passed() {
        let window = LogicFrames::from_frames(90);
        assert!(LastHurt(None).quiet_for(window, Frame::new(0)));
        let hurt = LastHurt(Some(Frame::new(100)));
        assert!(!hurt.quiet_for(window, Frame::new(189)));
        assert!(hurt.quiet_for(window, Frame::new(190)));
    }

    #[test]
    fn a_sole_healer_admits_itself_and_others_only_after_it_ends() {
        let (a, b) = (entity(1), entity(2));
        let held = Some(SoleHealer {
            by: a,
            until: Frame::new(15),
        });
        assert!(SoleHealer::admits(None, b, Frame::new(0)));
        assert!(SoleHealer::admits(held, a, Frame::new(10)));
        assert!(!SoleHealer::admits(held, b, Frame::new(15)));
        assert!(SoleHealer::admits(held, b, Frame::new(16)));
    }

    #[test]
    fn a_point_rule_adds_then_caps_by_the_scaled_cap() {
        let rule = PointRule {
            start: 100,
            cap: Some(1000),
            factor: 100,
            overridden: false,
        };
        assert_eq!(rule.limit(150), 250);
        assert_eq!(rule.limit(5000), 1000);
        assert_eq!(rule.limit(-500), 0, "never negative");
        let doubled = PointRule {
            factor: 200,
            ..rule
        };
        assert_eq!(doubled.limit(5000), 2000);
        assert_eq!(
            doubled.limit(150),
            250,
            "the factor scales the cap, not the sum"
        );
        let fixed = PointRule {
            start: 300,
            cap: None,
            factor: 100,
            overridden: false,
        };
        assert_eq!(fixed.limit(0), 300, "the alignment's limit, unchanged");
        assert_eq!(
            PointRule {
                start: i32::MAX,
                cap: None,
                factor: u32::MAX,
                overridden: false,
            }
            .limit(i32::MAX),
            i32::MAX,
            "saturates"
        );
    }

    #[test]
    fn ground_shots_count_toward_fired_but_not_a_targets_own_quota() {
        let mut hits = ArrowStormHits::default();
        hits.record(7);
        hits.record(7);
        hits.record_ground();
        hits.record(9);
        assert_eq!(hits.fired, 4);
        assert_eq!(hits.count(7), 2);
        assert_eq!(hits.count(9), 1);
        assert_eq!(hits.count(0), 0, "an untouched target has no count");
    }
}
