use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SortMode {
    pub(crate) key: usize,
    pub(crate) descending: bool,
}

impl SortMode {
    pub(crate) const fn ascending(key: usize) -> Self {
        Self {
            key,
            descending: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColumnSort {
    primary: SortMode,
    secondary: SortMode,
}

impl ColumnSort {
    pub(crate) const fn new(primary: SortMode, secondary: SortMode) -> Self {
        Self { primary, secondary }
    }

    pub(crate) fn click(&mut self, key: usize) {
        if self.primary.key == key {
            self.primary.descending = !self.primary.descending;
        } else {
            self.secondary = self.primary;
            self.primary = SortMode::ascending(key);
        }
    }

    pub(crate) fn compare(&self, by: impl Fn(usize) -> Ordering) -> Ordering {
        let step = |mode: SortMode| {
            let order = by(mode.key);
            if mode.descending {
                order.reverse()
            } else {
                order
            }
        };
        step(self.primary).then_with(|| step(self.secondary))
    }
}

pub(crate) fn name_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn mode(key: usize, descending: bool) -> SortMode {
        SortMode { key, descending }
    }

    #[test]
    fn a_click_flips_the_same_key_and_demotes_another() {
        let mut sort = ColumnSort::new(mode(1, false), mode(0, false));
        let steps = [
            (1, mode(1, true), mode(0, false)),
            (1, mode(1, false), mode(0, false)),
            (2, mode(2, false), mode(1, false)),
            (2, mode(2, true), mode(1, false)),
            (0, mode(0, false), mode(2, true)),
        ];
        for (key, primary, secondary) in steps {
            sort.click(key);
            assert_eq!(
                sort,
                ColumnSort::new(primary, secondary),
                "after clicking {key}"
            );
        }
    }

    #[test]
    fn a_primary_tie_falls_to_the_secondary_in_its_own_direction() {
        let rows = [(1, 'b'), (0, 'a'), (1, 'a'), (0, 'b')];
        let mut sort = ColumnSort::new(mode(0, false), mode(1, false));
        sort.click(1);
        sort.click(1);
        sort.click(0);
        let mut sorted = rows.to_vec();
        sorted.sort_by(|a, b| {
            sort.compare(|key| match key {
                0 => a.0.cmp(&b.0),
                _ => a.1.cmp(&b.1),
            })
        });
        assert_eq!(sorted, [(0, 'b'), (0, 'a'), (1, 'b'), (1, 'a')]);
    }
}
