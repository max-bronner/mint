use std::collections::BTreeMap;

use mint_sage_ini::Block;
use mint_sim::{UpgradeBit, UpgradeMask};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpgradeFilter {
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
    required_mask: UpgradeMask,
    forbidden_mask: UpgradeMask,
    required_unresolvable: bool,
    resolved: bool,
}

impl UpgradeFilter {
    #[must_use]
    pub fn read(block: &Block) -> Option<Self> {
        let list = |keyword: &str| -> Vec<String> {
            block
                .fields_named(keyword)
                .flat_map(|field| field.args.iter())
                .filter(|token| !token.trim().is_empty())
                .cloned()
                .collect()
        };
        let filter = Self {
            required: list("RequiredUpgradeNames"),
            forbidden: list("ForbiddenUpgradeNames"),
            ..Self::default()
        };
        (!filter.required.is_empty() || !filter.forbidden.is_empty()).then_some(filter)
    }

    pub fn resolve(&mut self, bit: &impl Fn(&str) -> Option<UpgradeBit>) {
        self.required_mask = UpgradeMask::EMPTY;
        self.forbidden_mask = UpgradeMask::EMPTY;
        self.required_unresolvable = false;
        for name in &self.required {
            match bit(name) {
                Some(bit) => self.required_mask.set(bit),
                None => self.required_unresolvable = true,
            }
        }
        for name in &self.forbidden {
            if let Some(bit) = bit(name) {
                self.forbidden_mask.set(bit);
            }
        }
        self.resolved = true;
    }

    #[must_use]
    pub fn passes(&self, held: &UpgradeMask) -> bool {
        if !self.resolved {
            return self.required.is_empty();
        }
        if !self.required.is_empty()
            && (self.required_unresolvable || !held.contains_all(&self.required_mask))
        {
            return false;
        }
        !held.intersects(&self.forbidden_mask)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpgradeFilters(BTreeMap<(usize, usize), UpgradeFilter>);

impl UpgradeFilters {
    pub fn insert(&mut self, weapon: usize, nugget: usize, filter: UpgradeFilter) {
        self.0.insert((weapon, nugget), filter);
    }

    pub fn clear_weapon(&mut self, weapon: usize) {
        self.0.retain(|(held, _), _| *held != weapon);
    }

    #[must_use]
    pub fn get(&self, weapon: usize, nugget: usize) -> Option<&UpgradeFilter> {
        self.0.get(&(weapon, nugget))
    }

    #[must_use]
    pub fn filters_weapon(&self, weapon: usize) -> bool {
        self.0
            .range((weapon, 0)..=(weapon, usize::MAX))
            .next()
            .is_some()
    }

    pub fn resolve(&mut self, bit: &impl Fn(&str) -> Option<UpgradeBit>) {
        for filter in self.0.values_mut() {
            filter.resolve(bit);
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn filter(required: &[&str], forbidden: &[&str]) -> UpgradeFilter {
        UpgradeFilter {
            required: required.iter().map(|s| (*s).to_owned()).collect(),
            forbidden: forbidden.iter().map(|s| (*s).to_owned()).collect(),
            ..UpgradeFilter::default()
        }
    }

    fn bits(name: &str) -> Option<UpgradeBit> {
        match name {
            "Fire" => Some(UpgradeBit::new(1)),
            "Other" => Some(UpgradeBit::new(2)),
            _ => None,
        }
    }

    fn held(indices: &[u32]) -> UpgradeMask {
        let mut mask = UpgradeMask::EMPTY;
        for index in indices {
            mask.set(UpgradeBit::new(*index));
        }
        mask
    }

    #[test]
    fn the_plain_and_fire_arrows_are_exclusive() {
        let mut plain = filter(&[], &["Fire"]);
        let mut fire = filter(&["Fire"], &[]);
        plain.resolve(&bits);
        fire.resolve(&bits);
        assert!(plain.passes(&held(&[])));
        assert!(!fire.passes(&held(&[])));
        assert!(!plain.passes(&held(&[1])));
        assert!(fire.passes(&held(&[1])));
    }

    #[test]
    fn any_forbidden_name_held_blocks() {
        let mut f = filter(&[], &["Other", "Fire"]);
        f.resolve(&bits);
        assert!(!f.passes(&held(&[1])));
        assert!(!f.passes(&held(&[2])));
        assert!(f.passes(&held(&[7])));
    }

    #[test]
    fn a_required_name_no_upgrade_declares_is_never_held() {
        let mut f = filter(&["Nobody"], &[]);
        f.resolve(&bits);
        assert!(!f.passes(&held(&[1, 2])));
    }

    #[test]
    fn an_unresolved_filter_keeps_the_first_versions_rule() {
        assert!(!filter(&["Fire"], &[]).passes(&held(&[1])));
        assert!(filter(&[], &["Fire"]).passes(&held(&[1])));
    }

    #[test]
    fn a_redefined_weapon_loses_its_old_filters() {
        let mut filters = UpgradeFilters::default();
        filters.insert(3, 0, filter(&["Fire"], &[]));
        filters.insert(3, 2, filter(&[], &["Fire"]));
        filters.insert(4, 0, filter(&["Fire"], &[]));
        assert!(filters.filters_weapon(3));
        filters.clear_weapon(3);
        assert!(!filters.filters_weapon(3));
        assert!(filters.filters_weapon(4));
        assert_eq!(filters.len(), 1);
    }
}
