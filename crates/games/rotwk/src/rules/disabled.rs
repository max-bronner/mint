use mint_sage_combat::nugget::ParalyzeNugget;
use mint_sage_modules::disabled_masks;
use mint_sage_modules::effects::names;
use mint_sage_modules::{Effect, PowerPlace};

use super::*;

const PARALYZED: &str = "PARALYZED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisabledKind {
    Paralysed,
    Frozen,
    TemporarilyBusy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Disabled {
    paralysed: Option<Frame>,
    frozen: Option<Frame>,
    busy: Option<Frame>,
    wears_paralyzed: bool,
}

impl Disabled {
    fn slot(&mut self, kind: DisabledKind) -> &mut Option<Frame> {
        match kind {
            DisabledKind::Paralysed => &mut self.paralysed,
            DisabledKind::Frozen => &mut self.frozen,
            DisabledKind::TemporarilyBusy => &mut self.busy,
        }
    }

    fn open(end: Option<Frame>, now: Frame) -> bool {
        end.is_some_and(|end| end > now)
    }

    fn shocked(&self, now: Frame) -> bool {
        Self::open(self.paralysed, now) || Self::open(self.frozen, now)
    }

    pub(crate) fn any(&self, now: Frame) -> bool {
        self.shocked(now) || Self::open(self.busy, now)
    }

    pub(crate) fn bits(&self, now: Frame) -> u32 {
        [
            (self.paralysed, disabled_masks::USER_PARALYZED),
            (self.frozen, disabled_masks::USER_FROZEN),
            (self.busy, disabled_masks::TEMPORARILY_BUSY),
        ]
        .into_iter()
        .filter(|(end, _)| Self::open(*end, now))
        .fold(0, |bits, (_, bit)| bits | bit)
    }
}

fn hash_end(end: Option<Frame>, hasher: &mut StateHasher) {
    hasher.write_bool(end.is_some());
    if let Some(end) = end {
        end.hash_into(hasher);
    }
}

impl Component for Disabled {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_end(self.paralysed, hasher);
        hash_end(self.frozen, hasher);
        hash_end(self.busy, hasher);
        hasher.write_bool(self.wears_paralyzed);
    }
}

impl RotwkRules {
    pub(super) fn ai_disabled(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        world
            .get(ids.disabled, entity)
            .is_some_and(|disabled| disabled.any(now))
    }

    pub(super) fn runs_when_disabled(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        now: Frame,
        mask: u32,
    ) -> bool {
        world
            .get(ids.disabled, entity)
            .map(|disabled| disabled.bits(now))
            .is_none_or(|bits| bits == 0 || bits & mask != 0)
    }

    pub(super) fn suspensions(
        &self,
        world: &World,
        ids: Ids,
        now: Frame,
    ) -> mint_sim::module::Suspensions {
        mint_sim::module::Suspensions::from_pairs(
            world
                .store(ids.disabled)
                .into_iter()
                .flat_map(|store| store.iter())
                .map(|(entity, disabled)| (entity, disabled.bits(now))),
        )
    }

    pub(super) fn disable_until(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: DisabledKind,
        until: Frame,
    ) -> Result<()> {
        let now = tick.frame();
        if until <= now {
            return Ok(());
        }
        let mut disabled = tick
            .state()
            .get(ids.disabled, entity)
            .copied()
            .unwrap_or_default();
        let slot = disabled.slot(kind);
        if slot.is_none_or(|held| held < until) {
            *slot = Some(until);
        }
        let shows = kind != DisabledKind::TemporarilyBusy && !disabled.wears_paralyzed;
        disabled.wears_paralyzed |= shows;
        tick.state_mut().insert(ids.disabled, entity, disabled)?;
        if shows {
            self.apply_effects(tick, ids, vec![(entity, paralyzed(true))])?;
        }
        Ok(())
    }

    pub(super) fn step_disabled(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let held: Vec<Entity> = tick
            .state()
            .store(ids.disabled)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let now = tick.frame();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for entity in held {
            let Some(mut disabled) = tick.state().get(ids.disabled, entity).copied() else {
                continue;
            };
            for end in [
                &mut disabled.paralysed,
                &mut disabled.frozen,
                &mut disabled.busy,
            ] {
                if end.is_some_and(|held| held <= now) {
                    *end = None;
                }
            }
            if disabled.wears_paralyzed && !disabled.shocked(now) {
                disabled.wears_paralyzed = false;
                effects.push((entity, paralyzed(false)));
            }
            if disabled == Disabled::default() {
                tick.state_mut().remove(ids.disabled, entity);
            } else {
                tick.state_mut().insert(ids.disabled, entity, disabled)?;
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn enter_damage_rung(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        before: crate::combat::DamageRung,
        after: crate::combat::DamageRung,
    ) -> Result<()> {
        use crate::combat::DamageRung;
        if before == after {
            return Ok(());
        }
        let Some(record) = self.record_of(tick, ids, victim) else {
            return Ok(());
        };
        let time = match after {
            DamageRung::Damaged => record.entering_damaged_transition_ms,
            DamageRung::ReallyDamaged => record.entering_really_damaged_transition_ms,
            DamageRung::Pristine | DamageRung::Rubble => None,
        };
        let Some(ms) = time.filter(|ms| ms.is_finite() && *ms > 0.0) else {
            return Ok(());
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let window = mint_sage_modules::retail_duration_ticks(ms.min(f32::from(u16::MAX)) as u32);
        let until = tick.frame().checked_add(window).unwrap_or(Frame::FOREVER);
        self.disable_until(tick, ids, victim, DisabledKind::TemporarilyBusy, until)
    }

    pub(super) fn paralyse(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        victim: Entity,
        nugget: &ParalyzeNugget,
    ) -> Result<()> {
        if nugget.affect_horde_members {
            let members: Vec<Entity> = tick
                .state()
                .get(ids.horde, victim)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for member in members {
                self.paralyse_one(tick, ids, source, member, nugget)?;
            }
        }
        self.paralyse_one(tick, ids, source, victim, nugget)?;
        self.evict_garrison(tick, ids, victim)
    }

    fn paralyse_one(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        victim: Entity,
        nugget: &ParalyzeNugget,
    ) -> Result<()> {
        let (Some(at), Some(body)) = (
            tick.state().get(ids.body, source).copied(),
            tick.state().get(ids.body, victim).copied(),
        ) else {
            return Ok(());
        };
        if source != victim
            && !super::knockback::inside_arc(
                at.0.facing.to_vector(),
                body.0.position.minus(at.0.position),
                nugget.damage_arc,
                false,
            )
        {
            return Ok(());
        }
        let now = tick.frame();
        let until = now.checked_add(nugget.duration).unwrap_or(Frame::FOREVER);
        let kind = if nugget.freeze_animation {
            DisabledKind::Frozen
        } else {
            DisabledKind::Paralysed
        };
        self.disable_until(tick, ids, victim, kind, until)?;
        if let Some(list) = &nugget.fx {
            self.apply_effects(
                tick,
                ids,
                vec![(
                    victim,
                    Effect::PlayFx {
                        list: list.as_str().into(),
                        at: PowerPlace::Source,
                        on_object: false,
                        secondary: None,
                    },
                )],
            )?;
        }
        Ok(())
    }

    fn evict_garrison(&self, tick: &mut Tick<'_>, ids: Ids, container: Entity) -> Result<()> {
        if !self
            .contain_view(tick.state(), container)
            .is_some_and(|view| view.garrison_family)
        {
            return Ok(());
        }
        let Some(list) = tick.state().get(ids.passengers, container).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        for passenger in list.0 {
            if tick.state().contains(ids.member, passenger) {
                continue;
            }
            let Some(mut held) = tick.state().get(ids.contained, passenger).copied() else {
                continue;
            };
            if held.leaves_at != Frame::FOREVER {
                continue;
            }
            held.leaves_at = now;
            tick.state_mut().insert(ids.contained, passenger, held)?;
        }
        Ok(())
    }
}

fn paralyzed(on: bool) -> Effect {
    let name = || names(vec![PARALYZED.to_owned()]);
    if on {
        Effect::ModelConditions {
            add: name(),
            remove: names(Vec::new()),
        }
    } else {
        Effect::ModelConditions {
            add: names(Vec::new()),
            remove: name(),
        }
    }
}
