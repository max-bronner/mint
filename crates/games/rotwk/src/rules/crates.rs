use mint_movement::Vec2;
use mint_sage_modules::SalvageCrateData;
use mint_sim::{Component, Entity, Frame, PlayerId, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UndetectedDefector(pub(crate) Frame);

impl Component for UndetectedDefector {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrateTextKind {
    Cash(i64),
    Rank,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CrateText {
    pub(crate) at: Vec2,
    pub(crate) player: PlayerId,
    pub(crate) kind: CrateTextKind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CrateTexts {
    frame: Frame,
    events: Vec<CrateText>,
}

impl CrateTexts {
    fn push(&mut self, now: Frame, event: CrateText) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    pub(crate) fn of(&self, now: Frame) -> &[CrateText] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for CrateTexts {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            hasher.write_f32(event.at.x);
            hasher.write_f32(event.at.y);
            hasher.write_u8(event.player.get());
            match event.kind {
                CrateTextKind::Cash(amount) => {
                    hasher.write_u8(0);
                    hasher.write_i64(amount);
                }
                CrateTextKind::Rank => hasher.write_u8(1),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reward {
    Level,
    Upgrade(String),
    Money(i64),
    Nothing,
}

impl RotwkRules {
    pub(super) fn run_crate_pickups(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
        largest_radius: f32,
        destroyed: &mut Vec<Entity>,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let crates: Vec<(Entity, SalvageCrateData)> = tick
            .state()
            .store(wiring.salvage_crate_state)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(entity, column)| {
                        column
                            .0
                            .first()
                            .map(|slot| (entity, slot.state.data().clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut near: Vec<u32> = Vec::new();
        for (crate_entity, data) in crates {
            if !tick.state().is_live(crate_entity)
                || tick.state().contains(ids.doomed, crate_entity)
                || destroyed.contains(&crate_entity)
            {
                continue;
            }
            let (Some(body), Some(shape)) = (
                tick.state().get(ids.body, crate_entity),
                tick.state().get(ids.shape, crate_entity),
            ) else {
                continue;
            };
            let (at, crate_radius) = (body.0.position, shape.radius);
            let invulnerable = tick
                .state()
                .get(ids.undetected_defector, crate_entity)
                .is_some_and(|until| tick.frame() < until.0);
            if invulnerable {
                continue;
            }
            let crate_owner = tick.state().get(ids.owner, crate_entity).map(|o| o.0);
            near.clear();
            grid.query_into(
                grid_cell(at.x),
                grid_cell(at.y),
                grid_radius_cells(crate_radius + largest_radius),
                &mut near,
            );
            near.sort_unstable();
            for &index in &near {
                let Some((foe, owner)) = usize::try_from(index).ok().and_then(|i| living.get(i))
                else {
                    continue;
                };
                if foe.entity == crate_entity || !touches(at, crate_radius, foe) {
                    continue;
                }
                if !self.may_take_crate(tick, ids, &data, foe.entity, *owner, crate_owner) {
                    continue;
                }
                if !data.allow_ai_pickup && self.is_computer(*owner) {
                    continue;
                }
                let reward = self.take_crate(tick, ids, &data, foe.entity, *owner)?;
                diag!(
                    "crate: entity {} takes crate entity {} for player {}: {reward:?}",
                    foe.entity.index(),
                    crate_entity.index(),
                    owner.get()
                );
                if let Some(fx) = &data.execute_fx {
                    self.emit_fx_object_named(
                        tick,
                        ids,
                        fx,
                        foe.position,
                        crate::fx::FxSource::Power,
                        foe.entity,
                        None,
                    );
                }
                let shown = match reward {
                    Reward::Level => Some(CrateTextKind::Rank),
                    Reward::Money(amount) => Some(CrateTextKind::Cash(amount)),
                    Reward::Nothing | Reward::Upgrade(_) => None,
                };
                if let Some(kind) = shown {
                    let now = tick.frame();
                    if let Some((_, texts)) = tick
                        .state_mut()
                        .store_mut(ids.crate_texts)
                        .and_then(|store| store.iter_mut().next())
                    {
                        texts.push(
                            now,
                            CrateText {
                                at,
                                player: *owner,
                                kind,
                            },
                        );
                    }
                }
                destroyed.push(crate_entity);
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn crate_texts<'w>(&self, world: &'w World, frame: Frame) -> &'w [CrateText] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.crate_texts)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, texts)| texts.of(frame))
    }

    pub(super) fn is_computer(&self, player: PlayerId) -> bool {
        player.get() >= self.players || self.ai_seeds.iter().any(|seed| seed.player == player)
    }

    fn may_take_crate(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        data: &SalvageCrateData,
        picker: Entity,
        owner: PlayerId,
        crate_owner: Option<PlayerId>,
    ) -> bool {
        let Some(record) = self.record_of(tick, ids, picker) else {
            return false;
        };
        if Some(owner) == self.neutral_player() {
            return false;
        }
        let has_ai = record
            .declared_modules
            .iter()
            .any(|module| mint_sage_modules::is_ai_module(&module.name));
        let building = data.building_pickup && record.has_kind("STRUCTURE");
        if !has_ai && !building {
            return false;
        }
        if record.has_kind("CREEP") && owner.get() >= self.players {
            return false;
        }
        if !data
            .required_kind_of
            .iter()
            .all(|kind| record.has_kind(kind))
            || data
                .forbidden_kind_of
                .iter()
                .any(|kind| record.has_kind(kind))
        {
            return false;
        }
        if data.forbid_owner_player && crate_owner == Some(owner) {
            return false;
        }
        if data.human_only && self.is_computer(owner) {
            return false;
        }
        if let Some(science) = &data.pickup_science {
            let held = self.sciences.id(science).is_some_and(|science| {
                self.science_of(tick.state(), ids, owner)
                    .is_some_and(|held| held.holds(science))
            });
            if !held {
                return false;
            }
        }
        has_ai && !record.has_kind("IMMOBILE")
    }

    fn take_crate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        data: &SalvageCrateData,
        picker: Entity,
        owner: PlayerId,
    ) -> Result<Reward> {
        if tick.rng().next_f32() < data.level_up_chance {
            self.gain_levels(tick, ids, picker, 1, None)?;
            return Ok(Reward::Level);
        }
        if let Some(upgrade) = &data.upgrade {
            match self.upgrade_bits.bit(upgrade) {
                Some(bit) => self.grant_player_upgrade(tick, ids, owner, bit)?,
                None => diag!("crate: {upgrade} is not an Upgrade - nothing granted"),
            }
            return Ok(Reward::Upgrade(upgrade.clone()));
        }
        let (min, max) = data.resource;
        let drawn = if min == max {
            i64::from(min)
        } else {
            let (low, high) = (i64::from(min.min(max)), i64::from(min.max(max)));
            let span =
                u32::try_from(high.saturating_sub(low).saturating_add(1)).unwrap_or(u32::MAX);
            low.saturating_add(i64::from(tick.rng().below(span)))
        };
        if drawn <= 0 {
            return Ok(Reward::Nothing);
        }
        let amount =
            crate::economy::deposited(drawn, self.multiplay.money(self.players), 1.0).max(0);
        if let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        {
            let mut purse = tick
                .state()
                .get(ids.purse, player_entity)
                .map_or(mint_sage_economy::Money::new(0), |held| held.0);
            purse.deposit(amount);
            tick.state_mut()
                .insert(ids.purse, player_entity, crate::economy::Purse(purse))?;
        }
        Ok(Reward::Money(amount))
    }
}

fn touches(at: Vec2, crate_radius: f32, foe: &Foe) -> bool {
    let (dx, dy) = (foe.position.x - at.x, foe.position.y - at.y);
    let reach = crate_radius + foe.radius;
    dx * dx + dy * dy <= reach * reach
}
