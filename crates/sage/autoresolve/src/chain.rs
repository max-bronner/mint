use std::collections::BTreeSet;

use crate::tables::UnitType;

pub type ChainStep = (UnitType, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Redefined(pub UnitType);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    named: BTreeSet<(i32, u8)>,
    unnamed: u32,
}

impl Default for Chain {
    fn default() -> Self {
        Self {
            named: BTreeSet::new(),
            unnamed: u32::MAX,
        }
    }
}

impl Chain {
    pub fn name(&mut self, target: UnitType, priority: i32) -> Result<(), Redefined> {
        let bit = 1u32 << target.index();
        if self.unnamed & bit == 0 {
            return Err(Redefined(target));
        }
        if priority != 0 {
            self.unnamed &= !bit;
            self.named.insert((priority, target.index_u8()));
        }
        Ok(())
    }

    fn first_unnamed_from(&self, from: usize) -> Option<UnitType> {
        (from..UnitType::ALL.len())
            .find(|i| self.unnamed & (1u32 << i) != 0)
            .and_then(UnitType::from_index)
    }

    fn step(entry: (i32, u8)) -> Option<ChainStep> {
        Some((UnitType::from_index(usize::from(entry.1))?, entry.0))
    }

    fn largest_negative(&self) -> Option<ChainStep> {
        self.named
            .iter()
            .next_back()
            .filter(|(pri, _)| *pri < 0)
            .copied()
            .and_then(Self::step)
    }

    #[must_use]
    pub fn first(&self) -> Option<ChainStep> {
        match self.named.iter().next_back() {
            Some(top) if top.0 > 0 => Self::step(*top),
            _ => self.first_unnamed_from(0).map(|t| (t, 0)),
        }
    }

    #[must_use]
    pub fn next(&self, target: UnitType, priority: i32) -> Option<ChainStep> {
        let from_named =
            |chain: &Self, key: (i32, u8)| chain.named.range(..key).next_back().copied();
        let key = (priority, target.index_u8());
        if priority > 0 {
            return match from_named(self, key) {
                Some(pred) if pred.0 > 0 => Self::step(pred),
                _ => self
                    .first_unnamed_from(0)
                    .map(|t| (t, 0))
                    .or_else(|| self.largest_negative()),
            };
        }
        if priority == 0 {
            return self
                .first_unnamed_from(target.index().saturating_add(1))
                .map(|t| (t, 0))
                .or_else(|| self.largest_negative());
        }
        from_named(self, key).and_then(Self::step)
    }
}
