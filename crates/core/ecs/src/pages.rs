use crate::to_usize;

pub(crate) const PAGE_SHIFT: u32 = 6;

pub(crate) const fn page_of(index: u32) -> u32 {
    index.wrapping_shr(PAGE_SHIFT)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DirtyPages {
    bits: Vec<u64>,
}

impl DirtyPages {
    pub(crate) const fn new() -> Self {
        Self { bits: Vec::new() }
    }

    pub(crate) fn mark(&mut self, page: u32) {
        let word = to_usize(page.wrapping_shr(6));
        let bit = 1u64.wrapping_shl(page & 63);
        if let Some(bits) = self.bits.get_mut(word) {
            *bits |= bit;
        } else {
            self.bits.resize(word.saturating_add(1), 0);
            if let Some(bits) = self.bits.get_mut(word) {
                *bits = bit;
            }
        }
    }

    pub(crate) fn drain(&mut self, mut visit: impl FnMut(usize)) {
        for (word_index, bits) in self.bits.iter_mut().enumerate() {
            let mut remaining = core::mem::take(bits);
            while remaining != 0 {
                visit(
                    word_index
                        .saturating_mul(64)
                        .saturating_add(to_usize(remaining.trailing_zeros())),
                );
                remaining &= remaining.wrapping_sub(1);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WriteStamps {
    count: u64,
    pages: Vec<u64>,
    cleared: u64,
}

impl WriteStamps {
    pub(crate) const fn new() -> Self {
        Self {
            count: 0,
            pages: Vec::new(),
            cleared: 0,
        }
    }

    pub(crate) fn mark(&mut self, page: u32) {
        self.count = self.count.saturating_add(1);
        let at = to_usize(page);
        if at >= self.pages.len() {
            self.pages.resize(at.saturating_add(1), 0);
        }
        if let Some(stamp) = self.pages.get_mut(at) {
            *stamp = self.count;
        }
    }

    pub(crate) fn clear_all(&mut self) {
        self.count = self.count.saturating_add(1);
        self.cleared = self.count;
    }

    pub(crate) const fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn since(&self, since: u64) -> impl Iterator<Item = u32> + '_ {
        let all = self.cleared > since;
        self.pages
            .iter()
            .enumerate()
            .filter(move |(_, stamp)| all || **stamp > since)
            .filter_map(|(page, _)| u32::try_from(page).ok())
    }
}
