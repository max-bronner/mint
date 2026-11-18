use mint_sim::StateHasher;

pub const GOOD_COMMAND_POINT_LIMIT: i32 = 300;

pub const EVIL_COMMAND_POINT_LIMIT: i32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Alignment {
    Good,
    Evil,
}

impl Alignment {
    #[must_use]
    pub const fn command_point_limit(self) -> i32 {
        match self {
            Self::Good => GOOD_COMMAND_POINT_LIMIT,
            Self::Evil => EVIL_COMMAND_POINT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandPointPool {
    used: i32,
    limit: i32,
}

impl CommandPointPool {
    #[must_use]
    pub const fn new(limit: i32) -> Self {
        Self {
            used: 0,
            limit: if limit < 0 { 0 } else { limit },
        }
    }

    #[must_use]
    pub const fn with_used(limit: i32, used: i32) -> Self {
        Self {
            used: if used < 0 { 0 } else { used },
            limit: if limit < 0 { 0 } else { limit },
        }
    }

    #[must_use]
    pub const fn for_alignment(alignment: Alignment) -> Self {
        Self::new(alignment.command_point_limit())
    }

    #[must_use]
    pub const fn used(self) -> i32 {
        self.used
    }

    #[must_use]
    pub const fn limit(self) -> i32 {
        self.limit
    }

    #[must_use]
    pub const fn free(self) -> i32 {
        let free = self.limit.saturating_sub(self.used);
        if free < 0 {
            0
        } else {
            free
        }
    }

    pub fn set_used(&mut self, used: i32) {
        self.used = used.max(0);
    }

    pub fn set_limit(&mut self, limit: i32) {
        self.limit = limit.max(0);
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_i32(self.used);
        hasher.write_i32(self.limit);
    }
}

impl core::fmt::Display for CommandPointPool {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.used, self.limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_faction_limits_are_the_shipped_numbers() {
        assert_eq!(Alignment::Good.command_point_limit(), 300);
        assert_eq!(Alignment::Evil.command_point_limit(), 600);
    }

    #[test]
    fn five_hordes_at_sixty_fill_a_good_players_limit_exactly() {
        let mut pool = CommandPointPool::for_alignment(Alignment::Good);
        pool.set_used(60 * 5);
        assert_eq!(pool.free(), 0);
        pool.set_used(60 * 4);
        assert_eq!(pool.free(), 60);
    }

    #[test]
    fn an_over_full_pool_reports_zero_free_rather_than_a_negative() {
        let pool = CommandPointPool::with_used(300, 400);
        assert_eq!(pool.free(), 0);
        assert_eq!(pool.used(), 400);
    }

    #[test]
    fn a_negative_limit_or_use_is_clamped_rather_than_wrapping() {
        assert_eq!(CommandPointPool::new(-10).limit(), 0);
        let mut pool = CommandPointPool::new(300);
        pool.set_used(-50);
        assert_eq!(pool.used(), 0);
        pool.set_limit(-1);
        assert_eq!(pool.limit(), 0);
        assert_eq!(pool.free(), 0);
    }

    #[test]
    fn the_hash_moves_with_both_halves() {
        let mut a = StateHasher::new();
        let mut b = StateHasher::new();
        let mut c = StateHasher::new();
        CommandPointPool::with_used(300, 60).hash_into(&mut a);
        CommandPointPool::with_used(300, 61).hash_into(&mut b);
        CommandPointPool::with_used(600, 60).hash_into(&mut c);
        assert_ne!(a, b);
        assert_ne!(a, c);
    }
}
