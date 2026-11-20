use mint_movement::Vec2;
use mint_sim::{Component, Entity, Frame, LogicFrames, Result, StateHasher, Tick, World};

use super::death::frames_of_ms;
use super::{Ids, RotwkRules};
use crate::combat::{armour_of, DamageEvent};
use crate::diag::diag;

const DEFAULT_FLANKED_DURATION_MS: f32 = 5000.0;

const MAX_REMEMBERED_ATTACKERS: usize = 16;

type Verdict = Option<Frame>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlankMemory {
    can_flank: bool,
    entries: Vec<(Entity, Verdict)>,
    was_attacking: bool,
}

impl Default for FlankMemory {
    fn default() -> Self {
        Self {
            can_flank: true,
            entries: Vec::new(),
            was_attacking: false,
        }
    }
}

impl Component for FlankMemory {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.can_flank);
        hasher.write_bool(self.was_attacking);
        hasher.write_usize(self.entries.len());
        for (attacker, verdict) in &self.entries {
            attacker.hash_into(hasher);
            hasher.write_bool(verdict.is_some());
            if let Some(expiry) = verdict {
                expiry.hash_into(hasher);
            }
        }
    }
}

impl FlankMemory {
    fn reset(&mut self) {
        self.can_flank = true;
        self.entries.clear();
    }

    fn cached(&self, attacker: Entity) -> Option<Verdict> {
        self.entries
            .iter()
            .find(|(id, _)| *id == attacker)
            .map(|(_, verdict)| *verdict)
    }

    fn remember(&mut self, attacker: Entity, verdict: Verdict) {
        if self.entries.iter().any(|(id, _)| *id == attacker) {
            return;
        }
        if self.entries.len() >= MAX_REMEMBERED_ATTACKERS {
            self.entries.remove(0);
        }
        self.entries.push((attacker, verdict));
    }
}

#[derive(Debug, Clone, Copy)]
struct Front {
    half_angle: f32,
    duration: LogicFrames,
}

impl Front {
    fn of(front_angle: Option<f32>, flanked_duration_ms: Option<f32>) -> Option<Self> {
        let angle = front_angle.filter(|angle| angle.is_finite() && *angle < 360.0)?;
        Some(Self {
            half_angle: angle.max(0.0) / 2.0,
            duration: frames_of_ms(flanked_duration_ms.unwrap_or(DEFAULT_FLANKED_DURATION_MS)),
        })
    }

    fn flanked_by(self, at: Vec2, facing: Vec2, attacker: Vec2) -> bool {
        !super::knockback::inside_arc(facing, attacker.minus(at), Some(self.half_angle), false)
    }
}

fn flank_bonus_factor(bonus: f32) -> f32 {
    1.0 + bonus
}

fn attack_just_ended(was_attacking: bool, attacking: bool) -> bool {
    was_attacking && !attacking
}

impl RotwkRules {
    pub(super) fn report_flank_census(&self) {
        let hordes = (0..self.kinds.len())
            .filter_map(|index| u16::try_from(index).ok())
            .filter_map(|index| self.kinds.get(crate::unit::Kind(index)))
            .filter(|entry| entry.record.front_angle.is_some_and(|angle| angle < 360.0))
            .count();
        let armours = self
            .combat
            .armors
            .iter()
            .filter(|armour| armour.flanked_penalty().is_some())
            .count();
        let (nuggets, flank_fx) =
            self.combat
                .weapons
                .iter()
                .fold((0usize, 0usize), |(nuggets, fx), (_, weapon)| {
                    let bonuses = weapon
                        .nuggets
                        .iter()
                        .filter(|nugget| {
                            matches!(nugget, mint_sage_combat::Nugget::Damage(damage)
                                if damage.flanking_bonus > 0.0)
                        })
                        .count();
                    (
                        nuggets.saturating_add(bonuses),
                        fx.saturating_add(usize::from(weapon.fire_flank_fx.is_some())),
                    )
                });
        println!(
            "flank: {hordes} horde kind(s) in this match can be flanked, {armours} armour(s) with a \
             FlankedPenalty, {nuggets} damage nugget(s) with a FlankingBonus, {flank_fx} \
             weapon(s) with a FireFlankFX"
        );
    }

    pub(super) fn strike_flank(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        event: &DamageEvent,
    ) -> Result<(f32, f32)> {
        let Some(victim_horde) = tick.state().get(ids.member, event.victim).map(|m| m.horde) else {
            return Ok((1.0, 0.0));
        };
        if !self.flanks(tick, ids, event.attacker, event.victim)? {
            return Ok((1.0, 0.0));
        }
        let armour = tick.state().get(ids.armour, event.victim).copied();
        let penalty = armour_of(&self.combat, armour, &self.unarmoured)
            .flanked_penalty()
            .unwrap_or(0.0);
        let factor = flank_bonus_factor(event.flanking_bonus);
        diag!(
            "flank: entity {} hit on horde {}'s flank by entity {}: amount x{:.2} \
             (FlankingBonus {:.0}%), armour weakened by FlankedPenalty {:.0}%",
            event.victim.index(),
            victim_horde.index(),
            event.attacker.index(),
            factor,
            event.flanking_bonus * 100.0,
            penalty * 100.0,
        );
        Ok((factor, penalty))
    }

    pub(super) fn flanked_scalar(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        event: &DamageEvent,
    ) -> Result<Option<f32>> {
        let Some(mint_sage_combat::Nugget::Damage(nugget)) = event
            .weapon
            .and_then(|weapon| self.combat.weapons.get(weapon))
            .and_then(|template| template.nuggets.get(usize::from(event.nugget)))
        else {
            return Ok(None);
        };
        if !self.flanks(tick, ids, event.victim, event.attacker)? {
            return Ok(None);
        }
        Ok(Some(nugget.shape.flanked_scalar.unwrap_or(1.0)))
    }

    pub(super) fn flanks(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Result<bool> {
        let world = tick.state();
        let Some(victim_horde) = world.get(ids.member, victim).map(|m| m.horde) else {
            return Ok(false);
        };
        let Some(attacker_horde) = world.get(ids.member, attacker).map(|m| m.horde) else {
            return Ok(false);
        };
        self.is_flanked_by(tick, ids, victim_horde, attacker_horde)
    }

    fn is_flanked_by(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim_horde: Entity,
        attacker_horde: Entity,
    ) -> Result<bool> {
        if !tick.state().is_live(victim_horde) || !tick.state().is_live(attacker_horde) {
            return Ok(false);
        }
        let Some(front) = self.front_of(tick.state(), ids, victim_horde) else {
            diag!(
                "flank: horde {} has no front (no FrontAngle under 360) - never flanked",
                victim_horde.index()
            );
            return Ok(false);
        };
        let attacker_memory = self.flank_memory(tick, ids, attacker_horde)?;
        if !attacker_memory.can_flank {
            diag!(
                "flank: horde {} cannot flank horde {} (attacker itself flanked)",
                attacker_horde.index(),
                victim_horde.index()
            );
            return Ok(false);
        }
        let mut victim_memory = self.flank_memory(tick, ids, victim_horde)?;
        let flanked = match victim_memory.cached(attacker_horde) {
            Some(verdict) => {
                let flanked = verdict.is_some_and(|expiry| tick.frame() < expiry);
                diag!(
                    "flank: cached verdict horde {} vs horde {}: flanked={}",
                    victim_horde.index(),
                    attacker_horde.index(),
                    flanked
                );
                flanked
            }
            None => {
                let world = tick.state();
                let (Some(at), Some(from)) = (
                    world.get(ids.body, victim_horde),
                    world.get(ids.body, attacker_horde),
                ) else {
                    diag!(
                        "flank: horde {} or horde {} has no body - never flanked",
                        victim_horde.index(),
                        attacker_horde.index()
                    );
                    return Ok(false);
                };
                let flanked =
                    front.flanked_by(at.0.position, at.0.facing.to_vector(), from.0.position);
                diag!(
                    "flank: geometry horde {} at {:?} facing {:?} vs horde {} at {:?}: half_angle {} flanked={}",
                    victim_horde.index(),
                    at.0.position,
                    at.0.facing.to_vector(),
                    attacker_horde.index(),
                    from.0.position,
                    front.half_angle,
                    flanked
                );
                if flanked {
                    let expiry = tick.frame().checked_add(front.duration);
                    victim_memory.remember(attacker_horde, Some(expiry.unwrap_or(Frame::FOREVER)));
                    victim_memory.can_flank = false;
                } else {
                    victim_memory.remember(attacker_horde, None);
                }
                flanked
            }
        };
        tick.state_mut()
            .insert(ids.flanked, victim_horde, victim_memory)?;
        Ok(flanked)
    }

    fn flank_memory(&self, tick: &mut Tick<'_>, ids: Ids, horde: Entity) -> Result<FlankMemory> {
        let world = tick.state();
        let mut memory = world.get(ids.flanked, horde).cloned().unwrap_or_default();
        let attacking =
            world.contains(ids.attack_order, horde) || world.contains(ids.horde_target, horde);
        if attack_just_ended(memory.was_attacking, attacking) {
            memory.reset();
        }
        memory.was_attacking = attacking;
        tick.state_mut()
            .insert(ids.flanked, horde, memory.clone())?;
        Ok(memory)
    }

    fn front_of(&self, world: &World, ids: Ids, horde: Entity) -> Option<Front> {
        let record = &self.kinds.get(*world.get(ids.kind, horde)?)?.record;
        Front::of(record.front_angle, record.flanked_duration_ms)
    }
}

pub(crate) fn flanked_amount(amount: f32, scalar: f32) -> Option<f32> {
    let scaled = amount * scalar;
    (scaled >= 1.0).then_some(scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DURATION: LogicFrames = LogicFrames::from_frames(60);

    #[test]
    fn a_strike_from_behind_flanks_and_one_from_the_front_never_does() {
        let front = Front {
            half_angle: 135.0,
            duration: DURATION,
        };
        let north = Vec2::new(0.0, 1.0);
        let at = Vec2::new(100.0, 100.0);
        assert!(front.flanked_by(at, north, Vec2::new(100.0, 0.0)));
        assert!(!front.flanked_by(at, north, Vec2::new(100.0, 200.0)));
        assert!(!front.flanked_by(at, north, Vec2::new(200.0, 100.0)));
        let narrow = Front {
            half_angle: 90.0,
            duration: DURATION,
        };
        assert!(narrow.flanked_by(at, north, Vec2::new(200.0, 90.0)));
    }

    #[test]
    fn a_front_of_360_or_none_is_never_flanked() {
        assert!(Front::of(Some(360.0), Some(2000.0)).is_none());
        assert!(Front::of(None, Some(2000.0)).is_none());
    }

    #[test]
    fn an_attack_ends_only_on_the_edge_from_attacking_to_not() {
        assert!(!attack_just_ended(false, false), "never attacked");
        assert!(!attack_just_ended(false, true), "just started");
        assert!(!attack_just_ended(true, true), "still attacking");
        assert!(attack_just_ended(true, false), "the actual edge");
    }

    #[test]
    fn the_bonus_multiplies_the_damage() {
        for (bonus, factor) in [(0.0, 1.0), (0.5, 1.5), (2.0, 3.0)] {
            assert!((flank_bonus_factor(bonus) - factor).abs() < 1e-6, "{bonus}");
        }
    }

    #[test]
    fn a_cached_verdict_does_not_re_test_geometry() {
        let mut world = World::new();
        let attacker = world.spawn().unwrap();
        let mut memory = FlankMemory::default();
        memory.remember(attacker, None);
        assert_eq!(memory.cached(attacker), Some(None));
        memory.remember(attacker, Some(Frame::new(1000)));
        assert_eq!(memory.cached(attacker), Some(None));
    }

    #[test]
    fn a_flanked_attackers_blow_under_one_deals_nothing() {
        assert_eq!(flanked_amount(0.5, 1.0), None);
        assert_eq!(flanked_amount(10.0, 0.5), Some(5.0));
        assert_eq!(flanked_amount(2.0, 1.0), Some(2.0));
    }

    #[test]
    fn can_flank_is_cleared_once_flanked_and_restored_on_reset() {
        let mut memory = FlankMemory::default();
        assert!(memory.can_flank);
        memory.can_flank = false;
        memory.reset();
        assert!(memory.can_flank);
        assert!(memory.entries.is_empty());
    }
}
