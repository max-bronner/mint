use mint_sim::StateHasher;

use crate::armor::ArmorTemplate;
use crate::damage::{DamageTypeId, DeathTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyKind {
    #[default]
    Active,
    Structure,
    Immortal,
    Highlander,
    Inactive,
}

impl BodyKind {
    #[must_use]
    pub fn parse(name: &str) -> Self {
        match name.trim().to_ascii_uppercase().as_str() {
            "STRUCTUREBODY" => Self::Structure,
            "IMMORTALBODY" => Self::Immortal,
            "HIGHLANDERBODY" => Self::Highlander,
            "INACTIVEBODY" => Self::Inactive,
            _ => Self::Active,
        }
    }

    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Active => 0,
            Self::Structure => 1,
            Self::Immortal => 2,
            Self::Highlander => 3,
            Self::Inactive => 4,
        }
    }

    #[must_use]
    pub const fn can_die(self) -> bool {
        matches!(self, Self::Active | Self::Structure | Self::Highlander)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthPool {
    current: f32,
    max: f32,
    initial: f32,
    kind: BodyKind,
    dead: bool,
    indestructible: bool,
    kill_scored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageOutcome {
    pub dealt: f32,
    pub clipped: f32,
    pub died: bool,
}

impl DamageOutcome {
    pub const NONE: Self = Self {
        dealt: 0.0,
        clipped: 0.0,
        died: false,
    };

    #[must_use]
    pub fn overkill_fraction(&self, max_health: f32) -> f32 {
        if !max_health.is_finite() || max_health <= 0.0 {
            return 0.0;
        }
        let wasted = self.dealt - self.clipped;
        if wasted.is_finite() && wasted > 0.0 {
            wasted / max_health
        } else {
            0.0
        }
    }
}

impl HealthPool {
    #[must_use]
    pub fn new(max_health: f32, kind: BodyKind) -> Option<Self> {
        if !max_health.is_finite() || max_health <= 0.0 {
            return None;
        }
        Some(Self {
            current: max_health,
            max: max_health,
            initial: max_health,
            kind,
            dead: false,
            indestructible: false,
            kill_scored: false,
        })
    }

    #[must_use]
    pub fn with_initial(mut self, initial: f32) -> Self {
        if initial.is_finite() && initial >= 0.0 {
            self.initial = initial;
            self.current = initial.min(self.max);
        }
        self
    }

    pub fn score_kill(&mut self) -> bool {
        std::mem::replace(&mut self.kill_scored, true)
    }

    pub fn set_indestructible(&mut self, indestructible: bool) {
        self.indestructible = indestructible;
    }

    #[must_use]
    pub const fn is_indestructible(&self) -> bool {
        self.indestructible
    }

    #[must_use]
    pub const fn current(&self) -> f32 {
        self.current
    }

    #[must_use]
    pub const fn max(&self) -> f32 {
        self.max
    }

    #[must_use]
    pub const fn kind(&self) -> BodyKind {
        self.kind
    }

    #[must_use]
    pub const fn is_dead(&self) -> bool {
        self.dead
    }

    #[must_use]
    pub fn fraction(&self) -> f32 {
        if !self.max.is_finite() || self.max <= 0.0 {
            return 0.0;
        }
        let f = self.current / self.max;
        if !f.is_finite() || f < 0.0 {
            0.0
        } else if f > 1.0 {
            1.0
        } else {
            f
        }
    }

    pub fn set_current(&mut self, value: f32) {
        if !value.is_finite() {
            return;
        }
        self.current = value.clamp(0.0, self.max);
    }

    pub fn set_initial_percent(&mut self, percent: f32) {
        if !percent.is_finite() || self.dead {
            return;
        }
        let initial = self.initial;
        let factor = percent / 100.0;
        if percent > 100.0 {
            let scaled = self.max * factor;
            if !scaled.is_finite() || scaled <= 0.0 {
                return;
            }
            self.max = scaled;
        }
        self.current = (factor * initial).clamp(0.0, self.max);
    }

    pub fn apply_health_modifier(&mut self, add: f32, mult: f32) {
        if self.dead {
            return;
        }
        let max = (self.max + add) * mult;
        let current = (self.current + add) * mult;
        if !max.is_finite() || max <= 0.0 || !current.is_finite() {
            return;
        }
        self.max = max;
        self.initial = max;
        self.current = current.clamp(0.0, max);
    }

    pub fn attempt_damage(
        &mut self,
        armour: &ArmorTemplate,
        damage_type: DamageTypeId,
        amount: f32,
        scalar: f32,
    ) -> DamageOutcome {
        self.attempt_damage_flanked(armour, damage_type, amount, scalar, 0.0)
    }

    pub fn attempt_damage_flanked(
        &mut self,
        armour: &ArmorTemplate,
        damage_type: DamageTypeId,
        amount: f32,
        scalar: f32,
        flanked_penalty: f32,
    ) -> DamageOutcome {
        if self.indestructible || self.dead || self.kind == BodyKind::Inactive {
            return DamageOutcome::NONE;
        }
        let mut adjusted = armour.adjust_damage_flanked(damage_type, amount, flanked_penalty);
        if scalar.is_finite() && scalar >= 0.0 {
            adjusted *= scalar;
        }
        if !adjusted.is_finite() || adjusted <= 0.0 {
            return DamageOutcome::NONE;
        }
        if matches!(self.kind, BodyKind::Immortal | BodyKind::Highlander) {
            let survivable = self.current - 1.0;
            if adjusted > survivable {
                adjusted = if survivable > 0.0 { survivable } else { 0.0 };
            }
            if adjusted <= 0.0 {
                return DamageOutcome::NONE;
            }
        }
        let previous = self.current;
        self.current = previous - adjusted;
        if !self.current.is_finite() || self.current < 0.0 {
            self.current = 0.0;
        }
        let outcome = DamageOutcome {
            dealt: adjusted,
            clipped: previous - self.current,
            died: self.current <= 0.0 && previous > 0.0 && self.kind.can_die(),
        };
        if outcome.died {
            self.dead = true;
        }
        outcome
    }

    pub fn kill(&mut self) -> bool {
        if self.dead || !self.kind.can_die() {
            return false;
        }
        self.current = 0.0;
        self.dead = true;
        true
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.current);
        hasher.write_f32(self.max);
        if self.initial.to_bits() != self.max.to_bits() {
            hasher.write_f32(self.initial);
        }
        hasher.write_u8(self.kind.code());
        hasher.write_bool(self.dead);
        if self.indestructible {
            hasher.write_bool(true);
        }
        if self.kill_scored {
            hasher.write_u8(2);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeathRecord {
    pub death_type: Option<DeathTypeId>,
}

impl DeathRecord {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match self.death_type {
            Some(id) => {
                hasher.write_u8(1);
                id.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::damage::DamageTypes;

    fn armour(pairs: &[(&str, f32)], types: &mut DamageTypes) -> ArmorTemplate {
        let rows: Vec<(String, f32)> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value))
            .collect();
        ArmorTemplate::from_rows(&rows, types)
    }

    #[test]
    fn set_current_clamps_and_ignores_non_finite_input_without_touching_dead() {
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        body.set_current(1.0);
        assert!((body.current() - 1.0).abs() < 1e-6);
        body.set_current(40.0);
        assert!(
            (body.current() - 40.0).abs() < 1e-6,
            "it can rise, not just fall"
        );
        body.set_current(500.0);
        assert!((body.current() - 100.0).abs() < 1e-6, "clamped at max");
        body.set_current(f32::NAN);
        assert!(
            (body.current() - 100.0).abs() < 1e-6,
            "a NaN input is ignored"
        );
        body.set_current(0.0);
        assert!((body.current()).abs() < 1e-6);
        assert!(
            !body.is_dead(),
            "only attempt_damage's edge trigger sets dead"
        );
    }

    #[test]
    fn damage_goes_through_the_armour_coefficient_then_the_scalar() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0), ("PIERCE", 0.5)], &mut types);
        let pierce = types.lookup("PIERCE").unwrap();
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let outcome = body.attempt_damage(&a, pierce, 40.0, 2.0);
        assert!((outcome.dealt - 40.0).abs() < 1e-3);
        assert!((body.current() - 60.0).abs() < 1e-3);
        assert!(!outcome.died);
    }

    #[test]
    fn health_falls_by_the_expected_amount_each_hit_and_death_fires_once() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0), ("SLASH", 0.25)], &mut types);
        let slash = types.lookup("SLASH").unwrap();
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let expected = [75.0f32, 50.0, 25.0, 0.0];
        let mut deaths = 0;
        for (hit, want) in expected.iter().enumerate() {
            let outcome = body.attempt_damage(&a, slash, 100.0, 1.0);
            assert!((body.current() - want).abs() < 1e-3, "hit {hit}");
            if outcome.died {
                deaths += 1;
            }
        }
        assert_eq!(deaths, 1);
        let after = body.attempt_damage(&a, slash, 100.0, 1.0);
        assert_eq!(after, DamageOutcome::NONE);
        assert_eq!(body.current(), 0.0);
    }

    #[test]
    fn overkill_is_the_damage_the_health_pool_could_not_absorb() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0)], &mut types);
        let crush = types.intern("CRUSH").unwrap();
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let outcome = body.attempt_damage(&a, crush, 300.0, 1.0);
        assert!(outcome.died);
        assert!((outcome.dealt - 300.0).abs() < 1e-3);
        assert!((outcome.clipped - 100.0).abs() < 1e-3);
        assert!((outcome.overkill_fraction(body.max()) - 2.0).abs() < 1e-3);
        let mut exact = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let clean = exact.attempt_damage(&a, crush, 100.0, 1.0);
        assert!(clean.died);
        assert_eq!(clean.overkill_fraction(exact.max()), 0.0);
    }

    #[test]
    fn an_immortal_body_survives_on_one_hit_point_and_never_dies() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0)], &mut types);
        let fire = types.intern("FLAME").unwrap();
        for kind in [BodyKind::Immortal, BodyKind::Highlander] {
            let mut body = HealthPool::new(100.0, kind).unwrap();
            for _ in 0..10 {
                let outcome = body.attempt_damage(&a, fire, 1000.0, 1.0);
                assert!(!outcome.died, "{kind:?}");
            }
            assert!((body.current() - 1.0).abs() < 1e-3, "{kind:?}");
            assert!(!body.is_dead(), "{kind:?}");
        }
    }

    #[test]
    fn an_inactive_body_takes_no_damage_at_all() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0)], &mut types);
        let fire = types.intern("FLAME").unwrap();
        let mut body = HealthPool::new(1.0, BodyKind::Inactive).unwrap();
        assert_eq!(
            body.attempt_damage(&a, fire, 1000.0, 1.0),
            DamageOutcome::NONE
        );
        assert!(!body.is_dead());
    }

    #[test]
    fn a_hostile_max_health_is_refused_rather_than_making_an_immortal_object() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -100.0] {
            assert!(HealthPool::new(bad, BodyKind::Active).is_none(), "{bad}");
        }
        assert!(HealthPool::new(f32::MIN_POSITIVE, BodyKind::Active).is_some());
    }

    #[test]
    fn a_zero_scalar_deals_no_damage() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0)], &mut types);
        let fire = types.intern("FLAME").unwrap();
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        assert_eq!(
            body.attempt_damage(&a, fire, 50.0, 0.0),
            DamageOutcome::NONE
        );
    }

    #[test]
    fn a_hostile_damage_amount_or_scalar_leaves_health_finite() {
        let mut types = DamageTypes::new();
        let a = armour(&[("DEFAULT", 1.0)], &mut types);
        let fire = types.intern("FLAME").unwrap();
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        for (amount, scalar) in [
            (f32::NAN, 1.0f32),
            (f32::INFINITY, 1.0),
            (10.0, f32::NAN),
            (10.0, f32::INFINITY),
            (-50.0, 1.0),
            (10.0, -1.0),
        ] {
            let before = body.current();
            let outcome = body.attempt_damage(&a, fire, amount, scalar);
            assert!(body.current().is_finite(), "{amount} {scalar}");
            assert!(body.current() >= 0.0, "{amount} {scalar}");
            if !amount.is_finite() || amount <= 0.0 {
                assert_eq!(outcome, DamageOutcome::NONE, "{amount} {scalar}");
                assert_eq!(body.current(), before, "{amount} {scalar}");
            }
            assert!(!body.fraction().is_nan());
        }
        let mut fresh = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let outcome = fresh.attempt_damage(&a, fire, f32::INFINITY, 1.0);
        assert_eq!(outcome, DamageOutcome::NONE);
        assert_eq!(outcome.overkill_fraction(100.0), 0.0);
    }

    #[test]
    fn the_kill_flag_kills_a_mortal_body_once_and_never_an_immortal_one() {
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        assert!(body.kill());
        assert_eq!(body.current(), 0.0);
        assert!(!body.kill());
        let mut immortal = HealthPool::new(100.0, BodyKind::Immortal).unwrap();
        assert!(!immortal.kill());
        assert!((immortal.current() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn every_field_of_a_health_pool_changes_its_hash() {
        let base = HealthPool::new(100.0, BodyKind::Active).unwrap();
        let hash_of = |p: &HealthPool| {
            let mut hasher = StateHasher::new();
            p.hash_into(&mut hasher);
            hasher.finish()
        };
        let mut hurt = base;
        hurt.current = 50.0;
        let mut bigger = base;
        bigger.max = 200.0;
        let mut other = base;
        other.kind = BodyKind::Structure;
        let mut gone = base;
        gone.dead = true;
        for p in [hurt, bigger, other, gone] {
            assert_ne!(hash_of(&base), hash_of(&p), "{p:?}");
        }
    }

    #[test]
    fn a_body_name_reads_to_its_kind_and_an_unknown_one_reads_as_active() {
        assert_eq!(BodyKind::parse("ActiveBody"), BodyKind::Active);
        assert_eq!(BodyKind::parse("structurebody"), BodyKind::Structure);
        assert_eq!(BodyKind::parse("IMMORTALBODY"), BodyKind::Immortal);
        assert_eq!(BodyKind::parse("HighlanderBody"), BodyKind::Highlander);
        assert_eq!(BodyKind::parse("InactiveBody"), BodyKind::Inactive);
        assert_eq!(BodyKind::parse("HiveStructureBody"), BodyKind::Active);
    }

    #[test]
    fn a_health_modifier_raises_max_and_current_together() {
        let mut pool = HealthPool::new(100.0, BodyKind::Active).unwrap();
        pool.set_current(40.0);
        pool.apply_health_modifier(20.0, 1.0);
        assert_eq!((pool.current(), pool.max()), (60.0, 120.0));
        pool.apply_health_modifier(0.0, 0.5);
        assert_eq!((pool.current(), pool.max()), (30.0, 60.0));
        pool.apply_health_modifier(-1000.0, 1.0);
        assert_eq!(pool.max(), 60.0, "a non-positive maximum is refused");
        pool.apply_health_modifier(f32::NAN, 1.0);
        assert_eq!(pool.max(), 60.0);
        let mut dead = HealthPool::new(100.0, BodyKind::Active).unwrap();
        assert!(dead.kill());
        dead.apply_health_modifier(20.0, 1.0);
        assert_eq!(dead.max(), 100.0);
    }

    #[test]
    fn a_kill_is_scored_once() {
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        assert!(!body.score_kill());
        assert!(body.score_kill());
    }

    #[test]
    fn set_initial_percent_takes_its_share_of_the_initial_health() {
        let mut body = HealthPool::new(100.0, BodyKind::Active).unwrap();
        body.set_initial_percent(400.0);
        assert_eq!(body.max(), 400.0);
        body.set_initial_percent(50.0);
        assert_eq!(body.current(), 50.0);
        body.apply_health_modifier(50.0, 1.0);
        body.set_initial_percent(25.0);
        assert_eq!(body.current(), 112.5);
    }
}
