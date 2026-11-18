use mint_sim::StateHasher;

pub const MAX_NAMES: usize = 512;

pub const DEFAULT_TOKEN: &str = "DEFAULT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DamageTypeId(u16);

impl DamageTypeId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeathTypeId(u16);

impl DeathTypeId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

#[derive(Debug, Clone, Default)]
pub struct NameTable {
    names: Vec<String>,
    sorted: Vec<(String, u16)>,
    refused: usize,
}

impl NameTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> Option<u16> {
        let key = name.trim().to_ascii_uppercase();
        if key.is_empty() {
            return None;
        }
        if let Ok(at) = self.sorted.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            return self.sorted.get(at).map(|(_, id)| *id);
        }
        let Ok(id) = u16::try_from(self.names.len()) else {
            self.refused = self.refused.saturating_add(1);
            return None;
        };
        if self.names.len() >= MAX_NAMES {
            self.refused = self.refused.saturating_add(1);
            return None;
        }
        let at = self
            .sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .unwrap_or_else(|at| at);
        self.sorted.insert(at, (key.clone(), id));
        self.names.push(key);
        Some(id)
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<u16> {
        let key = name.trim().to_ascii_uppercase();
        self.sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .ok()
            .and_then(|at| self.sorted.get(at).map(|(_, id)| *id))
    }

    #[must_use]
    pub fn name(&self, id: u16) -> Option<&str> {
        self.names.get(id as usize).map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    #[must_use]
    pub const fn refused(&self) -> usize {
        self.refused
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

#[derive(Debug, Clone, Default)]
pub struct DamageTypes(NameTable);

impl DamageTypes {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> Option<DamageTypeId> {
        if name.trim().eq_ignore_ascii_case(DEFAULT_TOKEN) {
            return None;
        }
        self.0.intern(name).map(DamageTypeId)
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<DamageTypeId> {
        self.0.lookup(name).map(DamageTypeId)
    }

    #[must_use]
    pub fn name(&self, id: DamageTypeId) -> Option<&str> {
        self.0.name(id.0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub const fn refused(&self) -> usize {
        self.0.refused()
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        self.0.names()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DamageFxTypeId(u16);

impl DamageFxTypeId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

#[derive(Debug, Clone, Default)]
pub struct DamageFxTypeNames(NameTable);

impl DamageFxTypeNames {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> Option<DamageFxTypeId> {
        if name.trim().eq_ignore_ascii_case(DEFAULT_TOKEN) {
            return None;
        }
        self.0.intern(name).map(DamageFxTypeId)
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<DamageFxTypeId> {
        self.0.lookup(name).map(DamageFxTypeId)
    }

    #[must_use]
    pub fn name(&self, id: DamageFxTypeId) -> Option<&str> {
        self.0.name(id.0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        self.0.names()
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeathTypes(NameTable);

impl DeathTypes {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> Option<DeathTypeId> {
        self.0.intern(name).map(DeathTypeId)
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<DeathTypeId> {
        self.0.lookup(name).map(DeathTypeId)
    }

    #[must_use]
    pub fn name(&self, id: DeathTypeId) -> Option<&str> {
        self.0.name(id.0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn names(&self) -> &[String] {
        self.0.names()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_name_interns_once_and_is_matched_without_regard_to_case() {
        let mut types = DamageTypes::new();
        let pierce = types.intern("PIERCE").unwrap();
        assert_eq!(types.intern("pierce"), Some(pierce));
        assert_eq!(types.intern("  Pierce "), Some(pierce));
        assert_eq!(types.len(), 1);
        assert_eq!(types.name(pierce), Some("PIERCE"));
    }

    #[test]
    fn ids_are_first_seen_order_even_though_the_lookup_table_is_sorted() {
        let mut types = DamageTypes::new();
        let slash = types.intern("SLASH").unwrap();
        let pierce = types.intern("PIERCE").unwrap();
        let crush = types.intern("CRUSH").unwrap();
        assert_eq!(slash.index(), 0);
        assert_eq!(pierce.index(), 1);
        assert_eq!(crush.index(), 2);
        assert_eq!(types.lookup("SLASH"), Some(slash));
        assert_eq!(types.lookup("PIERCE"), Some(pierce));
        assert_eq!(types.lookup("CRUSH"), Some(crush));
        assert_eq!(types.lookup("FLAME"), None);
    }

    #[test]
    fn the_default_wildcard_is_not_a_damage_type() {
        let mut types = DamageTypes::new();
        assert_eq!(types.intern("DEFAULT"), None);
        assert_eq!(types.intern("default"), None);
        assert_eq!(types.len(), 0);
    }

    #[test]
    fn a_mod_naming_more_types_than_the_cap_is_refused_rather_than_grown() {
        let mut table = NameTable::new();
        for i in 0..MAX_NAMES {
            assert!(table.intern(&format!("TYPE_{i}")).is_some(), "{i}");
        }
        assert_eq!(table.len(), MAX_NAMES);
        assert_eq!(table.intern("ONE_TOO_MANY"), None);
        assert_eq!(table.len(), MAX_NAMES);
        assert_eq!(table.refused(), 1);
        assert!(table.intern("TYPE_0").is_some());
    }

    #[test]
    fn a_blank_name_interns_nothing() {
        let mut table = NameTable::new();
        assert_eq!(table.intern(""), None);
        assert_eq!(table.intern("   "), None);
        assert!(table.is_empty());
    }
}
