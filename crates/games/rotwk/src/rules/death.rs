use super::*;
use crate::death_phase::{DeathPhase, DeathPhaseEvent};

pub(crate) const NORMAL_DEATH: &str = "NORMAL";

const CRUSHED_DEATH: &str = "CRUSHED";
const DETONATED_DEATH: &str = "DETONATED";

const BURNING_DEATH_DAMAGE_TYPES: [&str; 2] = ["FLAME", "LOGICAL_FIRE"];

const FADED_DEATH: &str = "FADED";

const BURNING_DEATH_EXCLUDED_CONDITIONS: [&str; 3] = ["STONED", "CLIMBING", "RAPPELLING"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KillingBlow {
    pub(crate) killer: Option<Entity>,
    pub(crate) damage_type: Option<mint_sage_combat::DamageTypeId>,
    pub(crate) dealt: f32,
    pub(crate) death: DeathNaming,
    pub(crate) sub_type: Option<mint_sage_combat::DamageTypeId>,
}

impl Component for KillingBlow {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.killer {
            Some(killer) => {
                hasher.write_u8(1);
                killer.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.damage_type {
            Some(damage_type) => {
                hasher.write_u8(1);
                damage_type.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_f32(self.dealt);
        self.death.hash_into(hasher);
        match self.sub_type {
            Some(sub) => {
                hasher.write_u8(1);
                sub.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DyingIds {
    pub(crate) selected: ComponentId<Selected>,
    pub(crate) attack_order: ComponentId<AttackOrder>,
    pub(crate) path: ComponentId<Path>,
    pub(crate) armed: ComponentId<Armed>,
    pub(crate) doomed: ComponentId<Doomed>,
}

pub(crate) fn mark_dying(
    world: &mut World,
    ids: DyingIds,
    entity: Entity,
    keep_selectable: bool,
    doomed: Doomed,
) -> Result<()> {
    stand_down(world, ids, entity, keep_selectable);
    world.insert(ids.doomed, entity, doomed)?;
    Ok(())
}

fn death_conditions_of(flags: &[String]) -> mint_sage_scene::Conditions {
    use mint_sage_scene::Conditions;
    flags.iter().fold(Conditions::NONE, |acc, name| {
        acc.union(if name.eq_ignore_ascii_case("DEATH_1") {
            Conditions::DEATH_1
        } else if name.eq_ignore_ascii_case("DEATH_2") {
            Conditions::DEATH_2
        } else if name.eq_ignore_ascii_case("DEATH_3") {
            Conditions::DEATH_3
        } else if name.eq_ignore_ascii_case("DEATH_4") {
            Conditions::DEATH_4
        } else if name.eq_ignore_ascii_case("DEATH_5") {
            Conditions::DEATH_5
        } else {
            Conditions::NONE
        })
    })
}

fn stand_down(world: &mut World, ids: DyingIds, entity: Entity, keep_selectable: bool) {
    if !keep_selectable {
        world.remove(ids.selected, entity);
    }
    world.remove(ids.attack_order, entity);
    world.remove(ids.path, entity);
    world.remove(ids.armed, entity);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Burning {
    until: Frame,
    overkill: f32,
    pub(crate) origin: Vec2,
}

impl Component for Burning {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.until.hash_into(hasher);
        hasher.write_f32(self.overkill);
        hasher.write_f32(self.origin.x);
        hasher.write_f32(self.origin.y);
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BurningSpec {
    time: LogicFrames,
    fx: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BurningDeaths {
    per_kind: Vec<Option<BurningSpec>>,
    report: (usize, usize, usize),
}

impl BurningDeaths {
    pub(crate) fn build(kinds: &crate::templates::KindTable) -> Self {
        let bits = mint_sage_modules::UpgradeBits::default();
        let mut out = Self::default();
        for index in 0..kinds.len() {
            let entry = u16::try_from(index)
                .ok()
                .and_then(|k| kinds.get(crate::unit::Kind(k)));
            let spec = entry.and_then(|entry| {
                let module = entry
                    .record
                    .declared_modules
                    .iter()
                    .rev()
                    .find(|module| module.field("BurningDeathBehavior").is_some())?;
                let burning =
                    mint_sage_modules::body::burning_death_from_fields(&module.source(&bits))
                        .map_err(|_| out.report.2 = out.report.2.saturating_add(1))
                        .ok()??;
                out.report.0 = out.report.0.saturating_add(1);
                let time_ms = entry.record.ai.burning_death_time_ms;
                if time_ms.is_none() {
                    out.report.1 = out.report.1.saturating_add(1);
                }
                Some(BurningSpec {
                    time: frames_of_ms(time_ms.unwrap_or(0.0)),
                    fx: burning.fx,
                })
            });
            out.per_kind.push(spec);
        }
        out
    }

    fn get(&self, kind: crate::unit::Kind) -> Option<&BurningSpec> {
        self.per_kind.get(usize::from(kind.0))?.as_ref()
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "burning deaths: {} kind(s) burn on a BURNED death or a kill while alight ({} with no \
             BurningDeathTime), {} refused for an unreadable BurningDeathBehavior",
            self.report.0, self.report.1, self.report.2
        )
    }
}

pub(super) fn frames_of_ms(milliseconds: f32) -> LogicFrames {
    if !milliseconds.is_finite() || milliseconds <= 0.0 {
        return LogicFrames::ZERO;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = milliseconds.min(86_400_000.0) as u32;
    mint_sage_modules::retail_duration_ticks(whole)
}

fn bearing_to(world: &World, ids: Ids, entity: Entity, killer: Entity) -> Option<f32> {
    let victim = &world.get(ids.body, entity)?.0;
    let other = &world.get(ids.body, killer)?.0;
    let dx = other.position.x - victim.position.x;
    let dy = other.position.y - victim.position.y;
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let degrees = (mint_math::f32::atan2(dy, dx) - victim.facing.radians()).to_degrees();
    Some(degrees.rem_euclid(360.0))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CorpseSink {
    rate: f32,
    start: Frame,
}

impl Component for CorpseSink {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.rate);
        self.start.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CorpseFade {
    start: Frame,
    end: Frame,
}

impl CorpseFade {
    pub(crate) fn opacity(&self, now: Frame) -> f32 {
        if now <= self.start {
            return 1.0;
        }
        if now >= self.end {
            return 0.0;
        }
        let span = self.end.get().saturating_sub(self.start.get()).max(1);
        let elapsed = now.get().saturating_sub(self.start.get());
        #[allow(clippy::cast_precision_loss)]
        let fraction = elapsed as f32 / span as f32;
        1.0 - fraction
    }
}

impl Component for CorpseFade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.start.hash_into(hasher);
        self.end.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeathPhaseClock {
    midpoint: Frame,
    midpoint_done: bool,
    hit_ground_armed: bool,
}

impl Component for DeathPhaseClock {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.midpoint.hash_into(hasher);
        hasher.write_bool(self.midpoint_done);
        hasher.write_bool(self.hit_ground_armed);
    }
}

fn draw<'a>(rng: &mut mint_sim::Pcg32, lists: &'a [String]) -> Option<&'a String> {
    let count = u32::try_from(lists.len()).ok().filter(|count| *count > 0)?;
    lists.get(usize::try_from(rng.below(count)).ok()?)
}

impl RotwkRules {
    pub(super) fn run_death_phase(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        phase: DeathPhase,
    ) -> Result<()> {
        let world = tick.state();
        let Some(option) = world.get(ids.doomed, entity).and_then(|doomed| {
            self.death_option(world.get(ids.kind, entity).copied(), doomed.death_index)
        }) else {
            return Ok(());
        };
        let at = phase.index();
        let ocl = option.ocl.get(at).map_or(&[][..], Vec::as_slice);
        let weapons = option.weapon.get(at).map_or(&[][..], Vec::as_slice);
        self.emit_death_phase(tick, ids, entity, phase);
        if let Some(list) = draw(tick.rng(), ocl) {
            self.run_death_lists(tick, ids, entity, std::slice::from_ref(list))?;
        }
        if let Some(name) = draw(tick.rng(), weapons) {
            self.fire_death_weapon(tick, ids, entity, name)?;
        }
        Ok(())
    }

    fn fire_death_weapon(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        let Some(weapon) = self.combat.weapons.id(name) else {
            crate::diag::diag!("death: weapon {name} is not declared; nothing fires");
            return Ok(());
        };
        let world = tick.state();
        let (Some(at), owner) = (
            world.get(ids.body, entity).map(|body| body.0.position),
            world.get(ids.owner, entity).map(|owner| owner.0),
        ) else {
            return Ok(());
        };
        crate::diag::diag!("death: entity {} fires {name}", entity.index());
        self.queue_shot(
            tick,
            ids,
            crate::spell_state::PendingShot {
                source: entity,
                weapon,
                at,
                repeat: None,
                alive_only: false,
                reload_slot: None,
                owner_if_gone: owner,
                victim: None,
            },
        )
    }

    pub(super) fn run_death_phase_updates(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let world = tick.state();
        let due: Vec<(Entity, DeathPhaseClock)> = world
            .store(ids.death_phase_clock)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, clock)| {
                        world.contains(ids.doomed, *entity)
                            && (clock.hit_ground_armed
                                || (!clock.midpoint_done && now >= clock.midpoint))
                    })
                    .map(|(entity, clock)| (entity, *clock))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, mut clock) in due {
            let mut phases = Vec::new();
            if clock.hit_ground_armed && self.height_above_terrain(tick.state(), ids, entity) < 1.0
            {
                clock.hit_ground_armed = false;
                phases.push(DeathPhase::HitGround);
            }
            if !clock.midpoint_done && now >= clock.midpoint {
                clock.midpoint_done = true;
                phases.push(DeathPhase::Midpoint);
            }
            tick.state_mut()
                .insert(ids.death_phase_clock, entity, clock)?;
            for phase in phases {
                self.run_death_phase(tick, ids, entity, phase)?;
            }
        }
        Ok(())
    }

    pub(super) fn object_z(&self, world: &World, ids: Ids, entity: Entity) -> f32 {
        let at = world
            .get(ids.body, entity)
            .map_or(Vec2::ZERO, |body| body.0.position);
        self.ground_height(at.x, at.y) + self.height_above_terrain(world, ids, entity)
    }

    pub(super) fn height_above_terrain(&self, world: &World, ids: Ids, entity: Entity) -> f32 {
        let Some(body) = world.get(ids.body, entity) else {
            return 0.0;
        };
        let at = body.0.position;
        let ground = self.ground_height(at.x, at.y);
        let base = world.get(ids.elevation, entity).map_or(ground, |e| e.0);
        base + world.get(ids.ground_offset, entity).map_or(0.0, |o| o.0) - ground
    }

    pub(super) fn emit_death_phase(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        phase: DeathPhase,
    ) {
        let world = tick.state();
        let (Some(kind), Some(death)) = (
            world.get(ids.kind, entity).copied(),
            world
                .get(ids.doomed, entity)
                .map(|doomed| doomed.death_index),
        ) else {
            return;
        };
        let Some(option) = self.death_option(Some(kind), death) else {
            return;
        };
        if !option.has_presentation(phase) {
            return;
        }
        let Some(at) = Self::fx_point_of(world, ids, entity) else {
            return;
        };
        let now = tick.frame();
        let Some(store) = tick.state_mut().store_mut(ids.death_phase) else {
            return;
        };
        if let Some((_, events)) = store.iter_mut().next() {
            events.push(
                now,
                DeathPhaseEvent {
                    entity,
                    kind,
                    death,
                    phase,
                    at,
                },
            );
            crate::diag::diag!(
                "death: entity {} {} phase, presentation to play one of fx {:?} and one of sound {:?}",
                entity.index(),
                phase.label(),
                option.fx.get(phase.index()).map_or(&[][..], Vec::as_slice),
                option.sound.get(phase.index()).map_or(&[][..], Vec::as_slice),
            );
        }
    }

    pub(crate) fn death_phase_events<'w>(
        &self,
        world: &'w World,
        frame: Frame,
    ) -> &'w [DeathPhaseEvent] {
        let Some(ids) = self.ids else {
            return &[];
        };
        world
            .store(ids.death_phase)
            .and_then(|store| store.iter().next())
            .map_or(&[], |(_, events)| events.of(frame))
    }

    pub(crate) fn death_phase_lists(&self, event: &DeathPhaseEvent) -> (&[String], &[String]) {
        let Some(option) = self.death_option(Some(event.kind), event.death) else {
            return (&[], &[]);
        };
        let at = event.phase.index();
        (
            option.fx.get(at).map_or(&[][..], Vec::as_slice),
            option.sound.get(at).map_or(&[][..], Vec::as_slice),
        )
    }

    pub(super) fn note_combat_fact(
        tick: &mut Tick<'_>,
        ids: Ids,
        write: impl FnOnce(&mut crate::combat_facts::CombatFacts, mint_sim::Frame),
    ) {
        let now = tick.frame();
        if let Some((_, facts)) = tick
            .state_mut()
            .store_mut(ids.combat_facts)
            .and_then(|store| store.iter_mut().next())
        {
            write(facts, now);
        }
    }

    fn firing_terms(&self, world: &World, ids: Ids, event: &DamageEvent) -> (f32, bool) {
        let Some(template) = event
            .weapon
            .and_then(|weapon| self.combat.weapons.get(weapon))
        else {
            return (1.0, false);
        };
        let share = self.passenger_share(world, ids, template, event.attacker);
        (share, template.use_innate_attributes)
    }

    pub(super) fn apply_damage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut [DamageEvent],
    ) -> Result<Vec<(Entity, f32)>> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        sort_damage(events);
        let mut killed = Vec::new();
        let mut kills: Vec<(Entity, Entity)> = Vec::new();
        let magic = self.combat.damage_types.lookup("MAGIC");
        let passenger_type = self.combat.damage_types.lookup("PASSENGER");
        let unresistable = self.combat.damage_types.lookup("UNRESISTABLE");
        let healing = self.combat.damage_types.lookup("HEALING");
        let now = tick.frame();
        for event in events.iter_mut() {
            if Some(event.damage_type) == healing {
                let base = tick
                    .state()
                    .get(ids.bonuses, event.attacker)
                    .filter(|_| !event.bonuses_applied)
                    .map_or(event.amount, |bonuses| {
                        bonuses
                            .0
                            .damage_of(event.amount, false, event.accepts_add, false)
                    });
                self.heal(tick, ids, event.victim, base * event.scalar, false, now)?;
                continue;
            }
            let Some(mut health) = tick.state().get(ids.health, event.victim).copied() else {
                continue;
            };
            if health.0.is_dead() {
                continue;
            }
            if tick
                .state()
                .get(ids.bonuses, event.victim)
                .is_some_and(|bonuses| bonuses.0.is_invulnerable_to(event.damage_type))
            {
                continue;
            }
            if tick.state().contains(ids.contained, event.victim)
                && self.container_shields(tick.state(), ids, event.victim)
            {
                let through = Some(event.damage_type) == passenger_type
                    || Some(event.damage_type) == unresistable;
                if !through {
                    continue;
                }
                if event.force_kill {
                    self.remove_from_contain(tick, ids, event.victim)?;
                }
            }
            let armour = tick.state().get(ids.armour, event.victim).copied();
            let (passenger_share, use_innate) = self.firing_terms(tick.state(), ids, event);
            let raw = event.amount * passenger_share;
            let base = tick
                .state()
                .get(ids.bonuses, event.attacker)
                .filter(|_| !event.bonuses_applied)
                .map_or(raw, |bonuses| {
                    bonuses.0.damage_of(
                        raw,
                        Some(event.damage_type) == magic,
                        event.accepts_add,
                        use_innate,
                    )
                });
            let base = tick
                .state()
                .get(ids.bonuses, event.victim)
                .map_or(base, |bonuses| {
                    base * bonuses.0.armor_factor(self.armor_bonus_cap)
                });
            let amount = if self.handicaps.is_empty() {
                base
            } else {
                let owner_of = |entity| tick.state().get(ids.owner, entity).map(|o| o.0);
                self.handicap_of(owner_of(event.attacker))
                    .damage_to(&self.handicap_of(owner_of(event.victim)), base)
            };
            let (flank_bonus, flank_penalty) = self.strike_flank(tick, ids, event)?;
            let amount = amount * flank_bonus;
            let amount = match self.flanked_scalar(tick, ids, event)? {
                None => amount,
                Some(scalar) => match crate::rules::flank::flanked_amount(amount, scalar) {
                    Some(amount) => amount,
                    None => continue,
                },
            };
            let before = health.0.fraction();
            let sacrifice = self.sacrifice_blow(tick.state(), ids, event.attacker, event.victim);
            let outcome = {
                let template = armour_of(
                    &self.combat,
                    if sacrifice { None } else { armour },
                    &self.unarmoured,
                );
                let (amount, scalar) = if sacrifice {
                    (health.0.max(), 1.0)
                } else {
                    (amount, event.scalar)
                };
                health.0.attempt_damage_flanked(
                    template,
                    event.damage_type,
                    amount,
                    scalar,
                    flank_penalty,
                )
            };
            let max = health.0.max();
            event.dealt = outcome.dealt;
            if outcome.dealt > 0.0 {
                Self::note_attacked_by(tick, ids, event.victim, event.attacker);
                Self::note_combat_fact(tick, ids, |facts, now| {
                    facts.push_struck(
                        now,
                        crate::combat_facts::Struck {
                            victim: event.victim,
                            attacker: event.attacker,
                            damage_type: event.damage_type,
                            dealt: outcome.dealt,
                        },
                    );
                });
            }
            tick.state_mut().insert(ids.health, event.victim, health)?;
            if outcome.dealt > 0.0 {
                self.pillage(tick, ids, event.attacker, event.victim)?;
            }
            if event.drain_life > 0.0
                && outcome.dealt > 0.0
                && !tick
                    .state()
                    .get(ids.shape, event.attacker)
                    .is_some_and(|shape| shape.kind_of.contains(KindOf::PROJECTILE))
            {
                self.heal(
                    tick,
                    ids,
                    event.attacker,
                    event.drain_life * outcome.dealt,
                    false,
                    now,
                )?;
            }
            let after = health.0.fraction();
            if let Some(launched) = event.launched {
                self.knock_side_off(tick, ids, event.victim, launched, before, after)?;
            }
            if !outcome.died {
                let thresholds = self.thresholds_of(tick.state(), ids, event.victim);
                self.enter_damage_rung(
                    tick,
                    ids,
                    event.victim,
                    thresholds.rung(before),
                    thresholds.rung(after),
                )?;
                if thresholds.rung(before) < crate::combat::DamageRung::ReallyDamaged
                    && thresholds.rung(after) >= crate::combat::DamageRung::ReallyDamaged
                {
                    self.fire_object_event(tick, ids, event.victim, "ReallyDamaged", None)?;
                }
                self.count_flame_hit(tick, ids, event.victim, event.damage_type, outcome.dealt)?;
            }
            self.play_damage_fx(tick, ids, event, outcome.dealt);
            self.note_hurt(tick, ids, event.victim);
            self.note_repulsor_damage(tick, ids, event.victim)?;
            self.note_emotion_hurt(tick, ids, event.victim);
            self.react_evacuate_damage(tick, ids, event.victim, event.weapon)?;
            self.react_horde_transport_contain_damage(tick, ids, event.victim)?;
            self.note_horde_combat(tick, ids, event.victim);
            self.note_horde_combat(tick, ids, event.attacker);
            if outcome.died {
                tick.state_mut().insert(
                    ids.killing_blow,
                    event.victim,
                    KillingBlow {
                        killer: Some(event.attacker),
                        damage_type: Some(event.damage_type),
                        dealt: outcome.dealt,
                        death: event.death,
                        sub_type: event.sub_type,
                    },
                )?;
                killed.push((event.victim, outcome.overkill_fraction(max)));
                kills.push((event.victim, event.attacker));
                if sacrifice {
                    self.note_sacrifice_kill(tick, ids, event.attacker, event.victim)?;
                }
                Self::note_combat_fact(tick, ids, |facts, now| {
                    facts.push_kill(now, event.victim, event.attacker);
                });
            }
        }
        self.award_kills(tick, ids, &kills)?;
        self.cheer_kills(tick, ids, &kills)?;
        Ok(killed)
    }

    pub(super) fn resolve_deaths(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: &[(Entity, f32)],
    ) -> Result<()> {
        let counted = |entity: &Entity| {
            !tick
                .state()
                .get(ids.shape, *entity)
                .is_some_and(|shape| shape.kind_of.contains(KindOf::PROJECTILE))
        };
        if killed.iter().map(|(entity, _)| entity).any(counted) {
            super::map_scripts::note_object_count_changed(tick, ids);
        }
        let with_castles: Vec<(Entity, f32)>;
        let castles = self.castle_deaths(tick, ids, killed)?;
        let killed = if castles.is_empty() {
            killed
        } else {
            with_castles = killed.iter().copied().chain(castles).collect();
            &with_castles
        };
        let with_containers: Vec<(Entity, f32)>;
        let cascade = self.contain_deaths(tick, ids, killed)?;
        let killed = if cascade.is_empty() {
            killed
        } else {
            with_containers = killed.iter().copied().chain(cascade).collect();
            &with_containers
        };
        let with_banners: Vec<(Entity, f32)>;
        let banner_cascade = self.banner_deaths(tick, ids, killed)?;
        let killed = if banner_cascade.is_empty() {
            killed
        } else {
            with_banners = killed.iter().copied().chain(banner_cascade).collect();
            &with_banners
        };
        self.note_deaths(tick, ids, killed);
        let faded_type = self.combat.death_types.lookup("FADED");
        for (entity, _) in killed {
            let faded = tick
                .state()
                .get(ids.killing_blow, *entity)
                .is_some_and(|blow| match blow.death {
                    DeathNaming::Faded => true,
                    DeathNaming::Named(id) => Some(id) == faded_type,
                    DeathNaming::Normal | DeathNaming::Crushed | DeathNaming::Detonated => false,
                });
            Self::note_combat_fact(tick, ids, |facts, now| {
                facts.push_death(now, *entity, faded);
            });
        }
        for (entity, _) in killed {
            if !tick.state().contains(ids.producer, *entity) {
                continue;
            }
            let Some(owner) = tick.state().get(ids.owner, *entity).map(|owner| owner.0) else {
                continue;
            };
            let refunded = self.cancel_all_production(tick, ids, *entity, owner)?;
            if refunded > 0 {
                crate::diag::diag!(
                    "death: entity {} refunded {refunded} for what it was producing",
                    entity.index()
                );
            }
        }
        for (entity, _) in killed {
            self.pay_refund_die(tick, ids, *entity)?;
            self.hero_die(tick, ids, *entity)?;
        }
        let placeholder = slow_death_options();
        let mut initial: Vec<Entity> = Vec::new();
        let mut death_fx: Vec<(Entity, mint_sage_modules::Effect)> = Vec::new();
        let mut kept: Vec<Entity> = Vec::new();
        for (entity, overkill) in killed {
            let now = tick.frame();
            let kind = tick.state().get(ids.kind, *entity).copied();
            let plan = kind
                .and_then(|kind| self.death_plans.get(kind))
                .unwrap_or_default();
            let held = self.effective_upgrades_in(tick.state(), ids, *entity);
            let sold: &[&str] = if tick.state().contains(ids.sold, *entity) {
                &[crate::rules::rebuild::SOLD_STATUS]
            } else {
                &[]
            };
            let dying = self.killing_blow(tick.state(), ids, *entity, sold, &held);
            let admitted: Vec<usize> = plan
                .iter()
                .enumerate()
                .filter(|(_, rung)| rung.mux.admits(&dying))
                .map(|(at, _)| at)
                .collect();
            let own: Vec<mint_sage_combat::SlowDeathOption> = admitted
                .iter()
                .filter_map(|at| plan.get(*at))
                .map(|rung| rung.option)
                .collect();
            let options: &[mint_sage_combat::SlowDeathOption] =
                if own.is_empty() { &placeholder } else { &own };
            let destroyed = kind.is_some_and(|kind| self.death_plans.destroys(kind, &dying));
            let (world, rng) = tick.state_and_rng_mut();
            let rolled = if destroyed && own.is_empty() {
                0
            } else {
                choose_slow_death(options, *overkill, rng).unwrap_or(0)
            };
            let picked = if own.is_empty() {
                plan.len().saturating_add(rolled)
            } else {
                admitted.get(rolled).copied().unwrap_or(rolled)
            };
            let chosen = plan.get(picked);
            crate::diag::diag!(
                "death: entity {} death={} rung {} of {} declared ({} admit it){}",
                entity.index(),
                dying.kind,
                picked,
                plan.len(),
                admitted.len(),
                if own.is_empty() {
                    " - the invented pair"
                } else {
                    ""
                }
            );
            let delay = options
                .get(rolled)
                .map_or(DEFAULT_DESTRUCTION_DELAY, |option| option.destruction_delay);
            if let Some(option) = options.get(rolled).filter(|option| option.sink_rate > 0.0) {
                let variance = rng.below(option.sink_delay_variance.get().saturating_add(1));
                let start = now
                    .checked_add(option.sink_delay)
                    .and_then(|frame| frame.checked_add(LogicFrames::from_frames(variance)))
                    .unwrap_or(Frame::FOREVER);
                let sink_id = ids.corpse_sink;
                world.insert(
                    sink_id,
                    *entity,
                    CorpseSink {
                        rate: option.sink_rate,
                        start,
                    },
                )?;
            }
            if let Some(death) = chosen {
                let frames = delay.get();
                let low = frames.saturating_mul(35) / 100;
                let high = frames.saturating_mul(65) / 100;
                let offset =
                    low.saturating_add(rng.below(high.saturating_sub(low).saturating_add(1)));
                world.insert(
                    ids.death_phase_clock,
                    *entity,
                    DeathPhaseClock {
                        midpoint: now
                            .checked_add(LogicFrames::from_frames(offset))
                            .unwrap_or(Frame::FOREVER),
                        midpoint_done: false,
                        hit_ground_armed: death.has_lists(DeathPhase::HitGround),
                    },
                )?;
                if death.has_lists(DeathPhase::Initial) {
                    initial.push(*entity);
                }
            }
            let record = kind
                .and_then(|kind| self.kinds.get(kind))
                .map(|entry| &entry.record);
            let rubble =
                !destroyed && own.is_empty() && record.is_some_and(|record| record.keeps_corpse);
            let mut destroy_at = if destroyed {
                now
            } else if rubble {
                Frame::FOREVER
            } else {
                now.checked_add(delay).unwrap_or(Frame::FOREVER)
            };
            if let Some(wiring) = self.modules.as_ref() {
                let mut scratch = mint_sim::module::DestroyRequests::new();
                let mut log = mint_sage_modules::EffectLog::new();
                let mut status: Vec<&str> = chosen
                    .map(|death| death.death_flags.iter().map(String::as_str).collect())
                    .unwrap_or_default();
                status.extend_from_slice(sold);
                if world.contains(ids.construction, *entity) {
                    status.push("UNDER_CONSTRUCTION");
                }
                let death = self.killing_blow(world, ids, *entity, &status, &held);
                wiring.broadcast_die(
                    world,
                    *entity,
                    now,
                    rng,
                    &mut scratch,
                    Effects::into_sink(&mut log),
                    death,
                );
                world.remove(ids.killing_blow, *entity);
                let height = world
                    .get(ids.shape, *entity)
                    .map_or(0.0, |shape| shape.height);
                if wiring.begin_collapse(world, *entity, self.gravity, height)
                    && own.is_empty()
                    && !destroyed
                {
                    destroy_at = Frame::FOREVER;
                }
                death_fx.extend(log.drain());
                wiring.product_died(world, *entity, now);
            }
            let keep_selectable =
                rubble && record.is_some_and(|record| record.keep_selectable_when_dead);
            if rubble {
                kept.push(*entity);
            }
            if let Some(fade_delay) = options.get(rolled).and_then(|option| option.fade_delay) {
                if destroy_at != Frame::FOREVER {
                    let start = now
                        .checked_add(fade_delay)
                        .unwrap_or(destroy_at)
                        .min(destroy_at);
                    world.insert(
                        ids.corpse_fade,
                        *entity,
                        CorpseFade {
                            start,
                            end: destroy_at,
                        },
                    )?;
                }
            }
            mark_dying(
                world,
                RotwkRules::dying_ids(ids),
                *entity,
                keep_selectable,
                Doomed {
                    destroy_at,
                    death_index: u32::try_from(picked).unwrap_or(u32::MAX),
                    death_conditions: chosen
                        .map(|death| death_conditions_of(&death.death_flags))
                        .unwrap_or(mint_sage_scene::Conditions::NONE),
                },
            )?;
        }
        self.apply_effects(tick, ids, death_fx)?;
        let died: Vec<Entity> = killed.iter().map(|(entity, _)| *entity).collect();
        self.expose_rebuild_holes(tick, ids, &died)?;
        self.turn_to_rubble(tick, ids, &died)?;
        for (entity, _) in killed {
            if let Some(repair) = tick.state().get(ids.auto_repair, *entity).copied() {
                tick.state_mut().remove(ids.auto_repair, *entity);
                if let crate::construction::Repairing::Worker(worker) = repair.repair {
                    self.release_site_worker(tick, ids, Some(worker))?;
                }
            }
            let Some(construction) = tick.state().get(ids.construction, *entity).copied() else {
                continue;
            };
            if kept.contains(entity) {
                tick.state_mut().remove(ids.construction, *entity);
            }
            self.release_site_worker(tick, ids, construction.worker)?;
            let Some(builder) = construction.builder else {
                continue;
            };
            self.release_builder(tick, ids, *entity, builder)?;
        }
        for entity in initial {
            self.run_death_phase(tick, ids, entity, DeathPhase::Initial)?;
        }
        self.note_hero_losses(tick, ids)
    }

    fn killing_blow<'a>(
        &'a self,
        world: &World,
        ids: Ids,
        entity: Entity,
        status: &'a [&'a str],
        held: &'a UpgradeMask,
    ) -> mint_sim::module::Death<'a> {
        let blow = world.get(ids.killing_blow, entity).copied();
        let name = |id| self.combat.damage_types.name(id).unwrap_or("");
        mint_sim::module::Death {
            kind: self.death_name(blow),
            cause: blow
                .and_then(|blow| blow.damage_type)
                .map(name)
                .unwrap_or(""),
            sub_cause: blow.and_then(|blow| blow.sub_type).map(name).unwrap_or(""),
            status,
            upgrades: Some(held),
            killer: blow.and_then(|blow| blow.killer),
            killer_bearing: blow
                .and_then(|blow| blow.killer)
                .and_then(|killer| bearing_to(world, ids, entity, killer)),
            amount: blow.map_or(0.0, |blow| blow.dealt),
        }
    }

    fn death_name(&self, blow: Option<KillingBlow>) -> &str {
        match blow.map(|blow| blow.death) {
            Some(DeathNaming::Crushed) => CRUSHED_DEATH,
            Some(DeathNaming::Detonated) => DETONATED_DEATH,
            Some(DeathNaming::Faded) => FADED_DEATH,
            Some(DeathNaming::Named(id)) => self
                .combat
                .death_types
                .name(id)
                .filter(|name| !name.is_empty())
                .unwrap_or(NORMAL_DEATH),
            Some(DeathNaming::Normal) | None => NORMAL_DEATH,
        }
    }

    fn is_burning_death_blow(&self, blow: Option<KillingBlow>) -> bool {
        let Some(damage_type) = blow.and_then(|blow| blow.damage_type) else {
            return false;
        };
        BURNING_DEATH_DAMAGE_TYPES
            .iter()
            .any(|name| self.combat.damage_types.lookup(name) == Some(damage_type))
    }

    pub(super) fn begin_burning_deaths(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        killed: Vec<(Entity, f32)>,
    ) -> Result<Vec<(Entity, f32)>> {
        let mut dying = Vec::with_capacity(killed.len());
        for (entity, overkill) in killed {
            let blow = tick.state().get(ids.killing_blow, entity).copied();
            let excluded = BURNING_DEATH_EXCLUDED_CONDITIONS
                .iter()
                .any(|name| self.holds_condition(tick, entity, name));
            let spec = tick
                .state()
                .get(ids.kind, entity)
                .and_then(|kind| self.burning_deaths.get(*kind))
                .filter(|_| !excluded && self.is_burning_death_blow(blow));
            let Some(spec) = spec else {
                dying.push((entity, overkill));
                continue;
            };
            let until = tick
                .frame()
                .checked_add(spec.time)
                .unwrap_or(Frame::FOREVER);
            let at = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position);
            let fx_at = Self::fx_point_of(tick.state(), ids, entity);
            tick.state_mut().insert(
                ids.burning,
                entity,
                Burning {
                    until,
                    overkill,
                    origin: at.unwrap_or(Vec2::ZERO),
                },
            )?;
            stand_down(tick.state_mut(), Self::dying_ids(ids), entity, false);
            self.apply_effects(
                tick,
                ids,
                vec![(
                    entity,
                    mint_sage_modules::effects::Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(vec!["BURNINGDEATH".to_owned()]),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                )],
            )?;
            diag!(
                "death: entity {} burning until frame {} ({} frame(s)), fx {}",
                entity.index(),
                until.get(),
                spec.time.get(),
                spec.fx.as_deref().unwrap_or("none"),
            );
            if let (Some(fx), Some(at)) = (spec.fx.as_deref(), fx_at) {
                let weapon = crate::fx::FxSource::Weapon;
                self.emit_fx_object_named(tick, ids, fx, at, weapon, entity, None);
            }
        }
        Ok(dying)
    }

    pub(super) fn end_burning_deaths(&self, tick: &Tick<'_>, ids: Ids) -> Vec<(Entity, f32)> {
        let now = tick.frame();
        let world = tick.state();
        world
            .store(ids.burning)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, burning)| {
                        burning.until <= now && !world.contains(ids.doomed, *entity)
                    })
                    .map(|(entity, burning)| (entity, burning.overkill))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn pay_refund_die(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let Some(refund) = wiring.refund_die(tick.state(), entity).cloned() else {
            return Ok(());
        };
        if refund.refund_percent() <= 0.0 || tick.state().contains(ids.sold, entity) {
            return Ok(());
        }
        let held = self.effective_upgrades_in(tick.state(), ids, entity);
        if !refund.admits(&self.killing_blow(tick.state(), ids, entity, &[], &held)) {
            return Ok(());
        }
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return Ok(());
        };
        if !refund.upgrade_required().is_empty()
            && !self
                .effective_upgrades_in(tick.state(), ids, entity)
                .contains_all(refund.upgrade_required())
        {
            return Ok(());
        }
        if !refund.building_required().is_empty() {
            let filter = ObjectFilter::parse(refund.building_required());
            if !self.owner_has_building(tick.state(), ids, owner, &filter) {
                return Ok(());
            }
        }
        let Some(kind) = tick.state().get(ids.kind, entity).copied() else {
            return Ok(());
        };
        let Some(record) = self.kinds.get(kind).map(|entry| &entry.record) else {
            return Ok(());
        };
        let modifiers = self.cost_modifiers_for_record(tick.state(), ids, owner, record);
        let amount = crate::construction::cost_refund(record, refund.refund_percent(), &modifiers);
        if amount <= 0 {
            return Ok(());
        }
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), owner)
        else {
            return Ok(());
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        purse.deposit(amount);
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        diag!(
            "death: entity {} refunded {amount} by RefundDie for player {}",
            entity.index(),
            owner.get()
        );
        Ok(())
    }

    fn owner_has_building(
        &self,
        world: &World,
        ids: Ids,
        owner: PlayerId,
        filter: &ObjectFilter,
    ) -> bool {
        world.store(ids.owner).is_some_and(|store| {
            store.iter().any(|(entity, held)| {
                held.0 == owner
                    && world.is_live(entity)
                    && !world.contains(ids.doomed, entity)
                    && world
                        .get(ids.kind, entity)
                        .and_then(|kind| self.kinds.get(*kind))
                        .is_some_and(|entry| {
                            filter.passes(&entry.record.kind_of, &entry.record.name)
                        })
            })
        })
    }

    pub(super) fn remove_corpses(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        destroyed: &[Entity],
    ) -> Result<()> {
        let now = tick.frame();
        self.sink_corpses(tick, ids, now)?;
        let corpses: Vec<Entity> = tick
            .state()
            .store(ids.doomed)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, doomed)| doomed.destroy_at <= now)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let mut due = corpses.clone();
        due.extend(
            destroyed
                .iter()
                .copied()
                .filter(|entity| tick.state().is_live(*entity)),
        );
        due.sort_unstable_by_key(|entity| entity.index());
        due.dedup();
        for &entity in &corpses {
            self.run_death_phase(tick, ids, entity, DeathPhase::Final)?;
        }
        if !due.is_empty() {
            if let Some(clock_entity) = tick
                .state()
                .store(ids.obstacles)
                .and_then(|store| store.entities().first().copied())
            {
                if let Some(mut layer) = tick.state().get(ids.obstacles, clock_entity).cloned() {
                    let geometry = self.passability.geometry;
                    let extent = self.passability.grid.extent();
                    for entity in &due {
                        layer.unstamp(&self.passability.grid, geometry, extent, *entity);
                    }
                    tick.state_mut()
                        .insert(ids.obstacles, clock_entity, layer)?;
                }
                if let Some(mut walls) =
                    tick.state().get(ids.scaleable_walls, clock_entity).cloned()
                {
                    for entity in &due {
                        walls.remove(*entity);
                    }
                    tick.state_mut()
                        .insert(ids.scaleable_walls, clock_entity, walls)?;
                }
            }
        }
        tick.state_mut().despawn_many(&due);
        if !due.is_empty() {
            super::map_scripts::note_object_count_changed(tick, ids);
        }
        Ok(())
    }

    fn sink_corpses(&self, tick: &mut Tick<'_>, ids: Ids, now: Frame) -> Result<()> {
        let sink_id = ids.corpse_sink;
        let due: Vec<(Entity, f32)> = tick
            .state()
            .store(sink_id)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, sink)| now >= sink.start)
                    .map(|(entity, sink)| (entity, sink.rate))
                    .collect()
            })
            .unwrap_or_default();
        if due.is_empty() {
            return Ok(());
        }
        let hz = f32::from(u16::try_from(mint_sim::SIMULATION_HZ).unwrap_or(30));
        for (entity, rate) in due {
            let current = tick
                .state()
                .get(ids.ground_offset, entity)
                .map_or(0.0, |offset| offset.0);
            tick.state_mut().insert(
                ids.ground_offset,
                entity,
                crate::unit::GroundOffset(current - rate / hz),
            )?;
        }
        Ok(())
    }
}
