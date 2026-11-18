use mint_sim::{LogicFrames, StateHasher};

use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoDeposit {
    timing: LogicFrames,
    amount: i64,
    claim: f32,
    elapsed: u32,
}

impl AutoDeposit {
    #[must_use]
    pub const fn new(timing: LogicFrames, amount: i64) -> Self {
        Self {
            timing,
            amount,
            claim: 1.0,
            elapsed: 0,
        }
    }

    #[must_use]
    pub const fn timing(&self) -> LogicFrames {
        self.timing
    }

    #[must_use]
    pub const fn amount(&self) -> i64 {
        self.amount
    }

    #[must_use]
    pub const fn claim(&self) -> f32 {
        self.claim
    }

    pub fn set_claim(&mut self, claim: f32) {
        self.claim = if claim.is_finite() {
            claim.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn restart(&mut self) {
        self.elapsed = 0;
    }

    #[must_use]
    pub fn frames_until_payment(&self) -> u32 {
        self.timing.get().saturating_sub(self.elapsed)
    }

    pub fn tick(&mut self, purse: &mut Money) -> i64 {
        self.elapsed = self.elapsed.saturating_add(1);
        if self.elapsed < self.timing.get() {
            return 0;
        }
        self.elapsed = 0;
        let payment = self.payment();
        purse.deposit(payment);
        payment
    }

    #[must_use]
    pub fn payment(&self) -> i64 {
        if self.amount <= 0 {
            return 0;
        }
        if self.claim >= 1.0 {
            return self.amount;
        }
        #[allow(clippy::cast_precision_loss)]
        let full = self.amount as f32;
        #[allow(clippy::cast_possible_truncation)]
        let scaled = (full * self.claim) as i64;
        scaled.max(0)
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        self.timing.hash_into(hasher);
        hasher.write_i64(self.amount);
        hasher.write_f32(self.claim);
        hasher.write_u32(self.elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_depositor_pays_on_its_interval_and_on_no_other_tick() {
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(LogicFrames::from_frames(10), 50);
        for frame in 1..=30u32 {
            let paid = deposit.tick(&mut purse);
            let due = frame % 10 == 0;
            assert_eq!(paid, if due { 50 } else { 0 }, "frame {frame} paid {paid}");
        }
        assert_eq!(purse.amount(), 150);
    }

    #[test]
    fn milliseconds_from_the_shipped_data_land_on_the_right_frame() {
        let timing = LogicFrames::from_milliseconds(1_000);
        assert_eq!(timing.get(), 30);
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(timing, 20);
        for _ in 0..29 {
            assert_eq!(deposit.tick(&mut purse), 0);
        }
        assert_eq!(deposit.tick(&mut purse), 20);
    }

    #[test]
    fn a_partial_claim_scales_the_payment_linearly_and_rounds_down() {
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(LogicFrames::from_frames(1), 100);
        deposit.set_claim(0.5);
        assert_eq!(deposit.tick(&mut purse), 50);
        deposit.set_claim(0.333);
        assert_eq!(deposit.tick(&mut purse), 33, "money is an integer");
        deposit.set_claim(0.0);
        assert_eq!(deposit.tick(&mut purse), 0);
    }

    #[test]
    fn a_broken_claim_pays_nothing_rather_than_everything() {
        let mut deposit = AutoDeposit::new(LogicFrames::from_frames(1), 100);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            deposit.set_claim(bad);
            assert_eq!(deposit.payment(), 0, "{bad} paid out");
        }
        deposit.set_claim(5.0);
        assert_eq!(
            deposit.payment(),
            100,
            "a claim above 1 is clamped, not scaled"
        );
        deposit.set_claim(-1.0);
        assert_eq!(deposit.payment(), 0);
    }

    #[test]
    fn a_zero_interval_pays_every_tick_rather_than_dividing_by_zero() {
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(LogicFrames::ZERO, 7);
        for _ in 0..5 {
            assert_eq!(deposit.tick(&mut purse), 7);
        }
        assert_eq!(purse.amount(), 35);
    }

    #[test]
    fn a_zero_or_negative_amount_pays_nothing() {
        let mut purse = Money::new(0);
        let mut nothing = AutoDeposit::new(LogicFrames::from_frames(1), 0);
        assert_eq!(nothing.tick(&mut purse), 0);
        let mut negative = AutoDeposit::new(LogicFrames::from_frames(1), -500);
        assert_eq!(negative.tick(&mut purse), 0);
        assert_eq!(purse.amount(), 0);
    }

    #[test]
    fn an_absurd_interval_never_pays_and_never_wraps() {
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(LogicFrames::MAX, 100);
        for _ in 0..1_000 {
            assert_eq!(deposit.tick(&mut purse), 0);
        }
        assert_eq!(deposit.frames_until_payment(), u32::MAX - 1_000);
    }

    #[test]
    fn the_hash_moves_with_the_timer_as_well_as_the_amount() {
        let mut purse = Money::new(0);
        let mut deposit = AutoDeposit::new(LogicFrames::from_frames(10), 50);
        let mut before = StateHasher::new();
        deposit.hash_into(&mut before);
        deposit.tick(&mut purse);
        let mut after = StateHasher::new();
        deposit.hash_into(&mut after);
        assert_ne!(before, after, "one tick of the timer must reach the hash");
    }
}
