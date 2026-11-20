use mint_sage_modules::CapturedModule;
use mint_sim::StateHasher;

use super::*;
use crate::defeat::ObjectFilter;

const CLICK_MODULE: &str = "ClickReactionBehavior";
const BORED_MODULE: &str = "BoredUpdate";
const MAX_RUNG: u32 = 5;
const RETAIL_FRAME: u64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClickReaction {
    pub(crate) counter: u32,
    pub(crate) playing: u32,
}

impl Component for ClickReaction {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.counter);
        hasher.write_u32(self.playing);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoredClock(pub(crate) Frame);

impl Component for BoredClock {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0.get());
    }
}

fn whole(module: &CapturedModule, keyword: &str) -> u32 {
    module
        .field(keyword)
        .and_then(|field| field.args.first())
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(0, |value| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = value.min(f64::from(u32::MAX)) as u32;
            whole
        })
}

fn react(rung: u32) -> String {
    format!("REACT_{rung}")
}

impl RotwkRules {
    fn declared<'a>(
        &'a self,
        world: &World,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Option<&'a CapturedModule> {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))?
            .record
            .declared_modules
            .iter()
            .find(|module| module.name.eq_ignore_ascii_case(name))
    }

    fn troll_holds(&self, world: &World, entity: Entity, name: &str) -> bool {
        self.modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .is_some_and(|state| state.has_model_condition(name))
    }

    fn at_rest(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        !Self::holds_attack(tick.state(), ids, entity)
            && !super::upgrades::is_moving(tick, ids, entity)
    }

    pub(super) fn click_reaction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        entity: Entity,
    ) -> Result<()> {
        let world = tick.state();
        if world.get(ids.owner, entity).map(|owner| owner.0) != Some(player) {
            return Ok(());
        }
        let Some(module) = self.declared(world, ids, entity, CLICK_MODULE) else {
            return Ok(());
        };
        if self.is_rampaging(world, entity)
            || self.troll_holds(world, entity, "RAMPAGE_ANIMATION_ONLY")
            || self.troll_holds(world, entity, "HIT_REACTION")
            || !self.at_rest(tick, ids, entity)
        {
            return Ok(());
        }
        let timer = whole(module, "ClickReactionTimer");
        let cap = timer.saturating_mul(MAX_RUNG);
        let mut held = world
            .get(ids.click_reaction, entity)
            .copied()
            .unwrap_or(ClickReaction {
                counter: 0,
                playing: 0,
            });
        held.counter = held.counter.saturating_add(timer).min(cap);
        let mut raised = None;
        if held.playing == 0 && timer > 0 {
            let mut rung = held.counter.checked_div(timer).unwrap_or(0);
            if rung <= 3 {
                rung = tick.rng().below(3).saturating_add(1);
            }
            if (1..=MAX_RUNG).contains(&rung) {
                held.playing = whole(module, &format!("ReactionFrames{rung}"));
                raised = Some(rung);
            }
        }
        tick.state_mut().insert(ids.click_reaction, entity, held)?;
        if let Some(rung) = raised {
            self.set_model_condition(tick, ids, entity, &react(rung), true)?;
        }
        Ok(())
    }

    fn end_reaction(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        for rung in 1..=6 {
            if self.troll_holds(tick.state(), entity, &react(rung)) {
                self.set_model_condition(tick, ids, entity, &react(rung), false)?;
            }
        }
        Ok(())
    }

    pub(super) fn step_troll_reactions(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        if tick.frame().get() % RETAIL_FRAME != 0 {
            return Ok(());
        }
        let mut clicked: Vec<Entity> = tick
            .state()
            .store(ids.click_reaction)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        clicked.sort_unstable_by_key(|entity| entity.index());
        for entity in clicked {
            let Some(mut held) = tick.state().get(ids.click_reaction, entity).copied() else {
                continue;
            };
            held.counter = held.counter.saturating_sub(1);
            if held.playing > 0 {
                held.playing = held.playing.saturating_sub(1);
                if held.playing == 0 || !self.at_rest(tick, ids, entity) {
                    held.playing = 0;
                    self.end_reaction(tick, ids, entity)?;
                }
            }
            tick.state_mut().insert(ids.click_reaction, entity, held)?;
        }
        self.step_bored(tick, ids)
    }

    fn step_bored(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let mut bored: Vec<Entity> = tick
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| tick.state().is_live(*entity))
                    .filter(|(entity, _)| {
                        self.declared(tick.state(), ids, *entity, BORED_MODULE)
                            .is_some()
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        bored.sort_unstable_by_key(|entity| entity.index());
        for entity in bored {
            let Some(module) = self.declared(tick.state(), ids, entity, BORED_MODULE) else {
                continue;
            };
            let delay = mint_sage_modules::retail_duration_ticks(whole(module, "ScanDelayTime"));
            let next = BoredClock(now.checked_add(delay).unwrap_or(Frame::FOREVER));
            let Some(due) = tick
                .state()
                .get(ids.bored_clock, entity)
                .map(|clock| clock.0)
            else {
                tick.state_mut().insert(ids.bored_clock, entity, next)?;
                continue;
            };
            if now < due {
                continue;
            }
            tick.state_mut().insert(ids.bored_clock, entity, next)?;
            self.bore(tick, ids, entity)?;
        }
        Ok(())
    }

    fn bore(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let world = tick.state();
        let Some(module) = self.declared(world, ids, entity, BORED_MODULE) else {
            return Ok(());
        };
        let Some(power) = module
            .field("SpecialPowerTemplate")
            .and_then(|field| field.args.first())
        else {
            return Ok(());
        };
        let scans_busy = module
            .field("CanScanWhileAttackingOrMoving")
            .and_then(|field| field.args.first())
            .is_some_and(|raw| raw.eq_ignore_ascii_case("Yes"));
        if !scans_busy && !self.at_rest(tick, ids, entity) {
            return Ok(());
        }
        if !self.power_is_ready(world, ids, tick.frame(), entity, power) {
            return Ok(());
        }
        let filter = module
            .field("BoredFilter")
            .filter(|field| !field.args.is_empty())
            .map(|field| ObjectFilter::parse(&field.args));
        let distance = module
            .field("ScanDistance")
            .and_then(|field| field.args.first())
            .and_then(|raw| raw.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        let Some(here) = world.get(ids.body, entity).map(|body| body.0.position) else {
            return Ok(());
        };
        let target = match &filter {
            None => None,
            Some(filter) => {
                let mut found: Vec<Entity> = world
                    .store(ids.kind)
                    .map(|store| store.entities().to_vec())
                    .unwrap_or_default();
                found.sort_unstable_by_key(|other| other.index());
                let pick = found.into_iter().find(|other| {
                    *other != entity
                        && world.is_live(*other)
                        && self.object_allied(world, entity, *other)
                        && world.get(ids.body, *other).is_some_and(|body| {
                            let (dx, dy) = (body.0.position.x - here.x, body.0.position.y - here.y);
                            dx * dx + dy * dy <= distance * distance
                        })
                        && world
                            .get(ids.kind, *other)
                            .and_then(|kind| self.kinds.get(*kind))
                            .is_some_and(|entry| {
                                filter.passes(&entry.record.kind_of, &entry.record.name)
                            })
                });
                match pick {
                    Some(other) => Some(other),
                    None => return Ok(()),
                }
            }
        };
        let power = power.clone();
        self.release_power(tick, ids, entity, &power, target, None, true)
    }
}
