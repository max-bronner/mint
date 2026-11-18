use mint_sim::{Component, Entity, Frame, StateHasher};

use crate::unit::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KillCredit {
    pub(crate) killer: Entity,
    pub(crate) victim: Kind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct KillCredits {
    frame: Frame,
    credits: Vec<KillCredit>,
}

impl KillCredits {
    pub(crate) fn push(&mut self, now: Frame, credit: KillCredit) {
        if self.frame != now {
            self.credits.clear();
            self.frame = now;
        }
        self.credits.push(credit);
    }

    pub(crate) fn of(&self, now: Frame) -> &[KillCredit] {
        if self.frame == now {
            &self.credits
        } else {
            &[]
        }
    }
}

impl Component for KillCredits {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.credits.len());
        for credit in &self.credits {
            credit.killer.hash_into(hasher);
            hasher.write_u16(credit.victim.0);
        }
    }
}
