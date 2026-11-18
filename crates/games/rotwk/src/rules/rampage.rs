use mint_sage_combat::WeaponStatus;
use mint_sim::{Component, Entity, Frame, LogicFrames, PlayerId, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::combat::{DamageEvent, DeathNaming, Foe};
use crate::diag::diag;
use crate::templates::KindTable;
use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq)]
struct RampageSpec {
    time: LogicFrames,
    requires_aflame: bool,
    eject_delay: Option<LogicFrames>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RampageSpecs(Vec<Option<RampageSpec>>);

impl RampageSpecs {
    pub(crate) fn build(kinds: &KindTable) -> Self {
        let mut out = Vec::with_capacity(kinds.len());
        for index in 0..kinds.len() {
            let spec = u16::try_from(index)
                .ok()
                .and_then(|k| kinds.get(Kind(k)))
                .filter(|entry| entry.record.ai.rampage_time_ms.is_some())
                .map(|entry| RampageSpec {
                    time: super::death::frames_of_ms(
                        entry.record.ai.rampage_time_ms.unwrap_or(0.0),
                    ),
                    requires_aflame: entry.record.ai.rampage_requires_aflame,
                    eject_delay: entry
                        .record
                        .ai
                        .eject_passengers_on_rampage_ms
                        .filter(|ms| ms.is_finite() && *ms > 0.0)
                        .map(super::death::frames_of_ms),
                });
            out.push(spec);
        }
        Self(out)
    }

    fn get(&self, kind: Kind) -> Option<RampageSpec> {
        self.0.get(usize::from(kind.0)).copied().flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rampaging;

impl Component for Rampaging {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingEject(Frame);

impl Component for PendingEject {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

impl RotwkRules {
    pub(super) fn enter_rampage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        if tick.state().contains(ids.rampaging, entity) || tick.state().contains(ids.doomed, entity)
        {
            return Ok(());
        }
        let Some(spec) = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.rampage_specs.get(kind))
        else {
            return Ok(());
        };
        let now = tick.frame();
        let aflame = self.holds_condition(tick, entity, "AFLAME");
        let rampage_time = if spec.requires_aflame && !aflame {
            LogicFrames::ZERO
        } else {
            spec.time
        };
        let ends_at = now.checked_add(rampage_time).unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(ids.rampaging, entity, Rampaging)?;
        tick.state_mut().insert(
            ids.lifetime,
            entity,
            crate::summons::Lifetime {
                dies_at: ends_at,
                death: DeathNaming::Normal,
                set_at: None,
            },
        )?;
        tick.state_mut().remove(ids.selected, entity);
        if let Some(delay) = spec.eject_delay {
            let eject_at = now.checked_add(delay).unwrap_or(Frame::FOREVER);
            tick.state_mut()
                .insert(ids.pending_eject, entity, PendingEject(eject_at))?;
        }
        self.apply_locomotor_override(tick, ids, entity, Some("ENRAGED"))?;
        self.apply_effects(
            tick,
            ids,
            vec![
                (
                    entity,
                    mint_sage_modules::effects::Effect::StatusBits {
                        set: mint_sage_modules::effects::names(vec![
                            "UNSELECTABLE".to_owned(),
                            "RAMPAGING".to_owned(),
                        ]),
                        clear: mint_sage_modules::effects::names(Vec::new()),
                    },
                ),
                (
                    entity,
                    mint_sage_modules::effects::Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(
                            vec!["WEAPONSET_RAMPAGE".to_owned()],
                        ),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                ),
                (
                    entity,
                    mint_sage_modules::effects::Effect::WeaponSetFlag("RAMPAGE".into()),
                ),
            ],
        )?;
        diag!(
            "rampage: entity {} enters RAMPAGING at frame {}, ends at frame {} ({} frame(s)){}",
            entity.index(),
            now.get(),
            ends_at.get(),
            rampage_time.get(),
            if spec.requires_aflame && !aflame {
                " (RampageRequiresAflame, not AFLAME: dies at once)"
            } else {
                ""
            }
        );
        Ok(())
    }

    pub(super) fn tick_rampage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let now = tick.frame();
        let entities: Vec<Entity> = tick
            .state()
            .store(ids.rampaging)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in entities {
            if let Some(PendingEject(eject_at)) =
                tick.state().get(ids.pending_eject, entity).copied()
            {
                if now >= eject_at {
                    self.schedule_exit(tick, ids, entity)?;
                    tick.state_mut().remove(ids.pending_eject, entity);
                }
            }
            let Some(armed) = tick.state().get(ids.armed, entity).copied() else {
                continue;
            };
            let Some(template) = self.combat.weapons.get(armed.weapon) else {
                continue;
            };
            let mut armed = armed;
            if armed.state.status(now) != WeaponStatus::ReadyToFire {
                continue;
            }
            let Some(at) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let weapon = armed.weapon;
            armed.state.fire(template, now, tick.rng());
            tick.state_mut().insert(ids.armed, entity, armed)?;
            diag!(
                "rampage: entity {} fires its rampage weapon at its own position ({:.0},{:.0})",
                entity.index(),
                at.x,
                at.y
            );
            self.fire_at_place(tick, ids, entity, weapon, at, None, events, living)?;
        }
        Ok(())
    }
}
