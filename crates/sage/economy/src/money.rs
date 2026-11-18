use mint_sim::StateHasher;

pub const REFUND_PERCENT: f32 = 50.0;

pub const DEFAULT_STARTING_CASH: i64 = 1_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Money(i64);

impl Money {
    #[must_use]
    pub const fn new(amount: i64) -> Self {
        Self(if amount < 0 { 0 } else { amount })
    }

    #[must_use]
    pub const fn amount(self) -> i64 {
        self.0
    }

    #[must_use]
    pub const fn can_afford(self, cost: i64) -> bool {
        cost <= 0 || cost <= self.0
    }

    pub fn deposit(&mut self, amount: i64) {
        if amount > 0 {
            self.0 = self.0.saturating_add(amount);
        }
    }

    pub fn withdraw(&mut self, amount: i64) -> i64 {
        if amount <= 0 {
            return 0;
        }
        let taken = amount.min(self.0);
        self.0 = self.0.saturating_sub(taken);
        taken
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_i64(self.0);
    }
}

impl core::fmt::Display for Money {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_withdrawal_larger_than_the_balance_takes_the_balance_and_says_so() {
        let mut purse = Money::new(120);
        assert_eq!(purse.withdraw(500), 120);
        assert_eq!(purse.amount(), 0);
        assert_eq!(purse.withdraw(1), 0);
    }

    #[test]
    fn a_purse_can_never_go_negative_or_wrap() {
        let mut purse = Money::new(0);
        assert_eq!(purse.withdraw(i64::MAX), 0);
        assert_eq!(purse.amount(), 0);
        assert_eq!(Money::new(-500).amount(), 0);

        let mut full = Money::new(i64::MAX);
        full.deposit(i64::MAX);
        assert_eq!(
            full.amount(),
            i64::MAX,
            "deposit saturates rather than wraps"
        );
    }

    #[test]
    fn a_negative_amount_is_ignored_on_both_sides() {
        let mut purse = Money::new(100);
        purse.deposit(-50);
        assert_eq!(purse.amount(), 100);
        assert_eq!(purse.withdraw(-50), 0);
        assert_eq!(purse.amount(), 100);
    }

    #[test]
    fn a_free_item_is_affordable_with_an_empty_purse() {
        let empty = Money::new(0);
        assert!(empty.can_afford(0));
        assert!(empty.can_afford(-10));
        assert!(!empty.can_afford(1));
    }

    #[test]
    fn the_hash_moves_with_the_total() {
        let mut a = StateHasher::new();
        let mut b = StateHasher::new();
        Money::new(1_500).hash_into(&mut a);
        Money::new(1_501).hash_into(&mut b);
        assert_ne!(a, b);
    }
}
