use mint_sim::StateHasher;

use crate::damage::{DamageTypeId, DamageTypes, DEFAULT_TOKEN};

const HEALING: &str = "HEALING";
const UNRESISTABLE: &str = "UNRESISTABLE";

pub const UNARMOURED: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ArmorTemplate {
    default: f32,
    overrides: Vec<(u16, f32)>,
    flanked_penalty: Option<f32>,
    damage_scalar: Option<f32>,
    healing: Option<u16>,
    unresistable: Option<u16>,
}

impl Default for ArmorTemplate {
    fn default() -> Self {
        Self {
            default: UNARMOURED,
            overrides: Vec::new(),
            flanked_penalty: None,
            damage_scalar: None,
            healing: None,
            unresistable: None,
        }
    }
}

impl ArmorTemplate {
    pub fn from_rows(rows: &[(String, f32)], types: &mut DamageTypes) -> Self {
        let mut template = Self::default();
        let key_of = |id: Option<DamageTypeId>| id.and_then(|id| u16::try_from(id.index()).ok());
        template.healing = key_of(types.intern(HEALING));
        template.unresistable = key_of(types.intern(UNRESISTABLE));
        for (token, coefficient) in rows {
            if token.trim().eq_ignore_ascii_case(DEFAULT_TOKEN) {
                template.default = *coefficient;
                template.overrides.clear();
                continue;
            }
            let Some(id) = types.intern(token) else {
                continue;
            };
            let key = u16::try_from(id.index()).unwrap_or(u16::MAX);
            match template.overrides.binary_search_by_key(&key, |(k, _)| *k) {
                Ok(at) => {
                    if let Some(slot) = template.overrides.get_mut(at) {
                        slot.1 = *coefficient;
                    }
                }
                Err(at) => template.overrides.insert(at, (key, *coefficient)),
            }
        }
        template
    }

    #[must_use]
    pub const fn flanked_penalty(&self) -> Option<f32> {
        self.flanked_penalty
    }

    #[must_use]
    pub const fn damage_scalar(&self) -> Option<f32> {
        self.damage_scalar
    }

    #[must_use]
    pub const fn with_scalars(mut self, flanked: Option<f32>, scalar: Option<f32>) -> Self {
        self.flanked_penalty = flanked;
        self.damage_scalar = scalar;
        self
    }

    #[must_use]
    pub fn coefficient(&self, damage_type: DamageTypeId) -> f32 {
        let key = u16::try_from(damage_type.index()).unwrap_or(u16::MAX);
        self.overrides
            .binary_search_by_key(&key, |(k, _)| *k)
            .ok()
            .and_then(|at| self.overrides.get(at))
            .map_or(self.default, |(_, coefficient)| *coefficient)
    }

    #[must_use]
    pub fn adjust_damage(&self, damage_type: DamageTypeId, amount: f32) -> f32 {
        self.adjust_damage_flanked(damage_type, amount, 0.0)
    }

    #[must_use]
    pub fn adjust_damage_flanked(
        &self,
        damage_type: DamageTypeId,
        amount: f32,
        flanked_penalty: f32,
    ) -> f32 {
        let key = u16::try_from(damage_type.index()).ok();
        if key.is_some() && key == self.healing {
            return if amount.is_finite() { amount } else { 0.0 };
        }
        let weaken = |factor: f32| {
            if flanked_penalty.is_finite() && flanked_penalty > 0.0 {
                1.0 - (1.0 - factor) * (1.0 - flanked_penalty)
            } else {
                factor
            }
        };
        let scalar = weaken(self.damage_scalar.unwrap_or(1.0));
        let coefficient = if key.is_some() && key == self.unresistable {
            1.0
        } else {
            weaken(self.coefficient(damage_type))
        };
        let adjusted = amount * scalar * coefficient;
        if adjusted.is_finite() && adjusted > 0.0 {
            adjusted
        } else {
            0.0
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.default);
        hasher.write_usize(self.overrides.len());
        for (key, coefficient) in &self.overrides {
            hasher.write_u16(*key);
            hasher.write_f32(*coefficient);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArmorStore {
    templates: Vec<ArmorTemplate>,
    sorted: Vec<(String, u32)>,
    dropped_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArmorId(u32);

impl ArmorId {
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

impl ArmorStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: &str, template: ArmorTemplate) -> Option<ArmorId> {
        let key = name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        match self.sorted.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            Ok(at) => {
                let id = self.sorted.get(at).map(|(_, id)| *id)?;
                *self.templates.get_mut(id as usize)? = template;
                Some(ArmorId(id))
            }
            Err(at) => {
                let id = u32::try_from(self.templates.len()).ok()?;
                self.templates.push(template);
                self.sorted.insert(at, (key, id));
                Some(ArmorId(id))
            }
        }
    }

    #[must_use]
    pub fn id(&self, name: &str) -> Option<ArmorId> {
        let key = name.trim().to_ascii_lowercase();
        self.sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .ok()
            .and_then(|at| self.sorted.get(at))
            .map(|(_, id)| ArmorId(*id))
    }

    #[must_use]
    pub fn get(&self, id: ArmorId) -> Option<&ArmorTemplate> {
        self.templates.get(id.index())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ArmorTemplate> {
        self.templates.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    #[must_use]
    pub const fn dropped_rows(&self) -> usize {
        self.dropped_rows
    }

    pub fn record_dropped_rows(&mut self, count: usize) {
        self.dropped_rows = self.dropped_rows.saturating_add(count);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, f32)]) -> Vec<(String, f32)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value))
            .collect()
    }

    #[test]
    fn a_default_row_after_a_type_row_overwrites_that_type() {
        let mut types = DamageTypes::new();
        let flame = types.intern("FLAME").unwrap();
        let template =
            ArmorTemplate::from_rows(&rows(&[("FLAME", 1.0), ("DEFAULT", 0.5)]), &mut types);
        assert!((template.coefficient(flame) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_type_row_after_a_default_row_overrides_only_that_type() {
        let mut types = DamageTypes::new();
        let template =
            ArmorTemplate::from_rows(&rows(&[("DEFAULT", 0.5), ("FLAME", 2.0)]), &mut types);
        let flame = types.lookup("FLAME").unwrap();
        let pierce = types.intern("PIERCE").unwrap();
        assert!((template.coefficient(flame) - 2.0).abs() < 1e-6);
        assert!((template.coefficient(pierce) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_type_nobody_named_is_unarmoured() {
        let mut types = DamageTypes::new();
        let template = ArmorTemplate::from_rows(&rows(&[("FLAME", 0.1)]), &mut types);
        let magic = types.intern("MAGIC").unwrap();
        assert!((template.coefficient(magic) - UNARMOURED).abs() < 1e-6);
    }

    #[test]
    fn a_duplicate_row_takes_the_last_value() {
        let mut types = DamageTypes::new();
        let template = ArmorTemplate::from_rows(
            &rows(&[("DEFAULT", 1.0), ("STRUCTURAL", 0.25), ("STRUCTURAL", 0.75)]),
            &mut types,
        );
        let structural = types.lookup("STRUCTURAL").unwrap();
        assert!((template.coefficient(structural) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn adjust_damage_multiplies_and_clamps_negatives_to_zero() {
        let mut types = DamageTypes::new();
        let template =
            ArmorTemplate::from_rows(&rows(&[("DEFAULT", 0.5), ("MAGIC", -1.0)]), &mut types);
        let pierce = types.intern("PIERCE").unwrap();
        let magic = types.lookup("MAGIC").unwrap();
        assert!((template.adjust_damage(pierce, 100.0) - 50.0).abs() < 1e-4);
        assert_eq!(template.adjust_damage(magic, 100.0), 0.0);
    }

    #[test]
    fn flanked_adjust_damage_weakens_the_coefficient_not_the_raw_damage() {
        let mut types = DamageTypes::new();
        let template = ArmorTemplate::from_rows(&rows(&[("DEFAULT", 0.5)]), &mut types);
        let pierce = types.intern("PIERCE").unwrap();
        assert!((template.adjust_damage_flanked(pierce, 100.0, 0.5) - 75.0).abs() < 1e-4);
        assert!(
            (template.adjust_damage_flanked(pierce, 100.0, 0.0)
                - template.adjust_damage(pierce, 100.0))
            .abs()
                < 1e-6
        );
        let full = ArmorTemplate::from_rows(&rows(&[("DEFAULT", 1.0)]), &mut types);
        assert!((full.adjust_damage_flanked(pierce, 100.0, 0.5) - 100.0).abs() < 1e-4);
    }

    #[test]
    fn the_armours_own_damage_scalar_multiplies_all_but_healing() {
        let mut types = DamageTypes::new();
        let template =
            ArmorTemplate::from_rows(&rows(&[("DEFAULT", 1.0), ("SLASH", 0.8)]), &mut types)
                .with_scalars(None, Some(0.5));
        let slash = types.lookup("SLASH").unwrap();
        let unresistable = types.lookup("UNRESISTABLE").unwrap();
        let healing = types.lookup("HEALING").unwrap();
        assert!((template.adjust_damage(slash, 100.0) - 40.0).abs() < 1e-4);
        assert!((template.adjust_damage(unresistable, 100.0) - 50.0).abs() < 1e-4);
        assert!((template.adjust_damage(healing, 100.0) - 100.0).abs() < 1e-4);
        assert!((template.adjust_damage_flanked(unresistable, 100.0, 0.5) - 75.0).abs() < 1e-4);
    }

    #[test]
    fn a_non_finite_coefficient_or_amount_yields_no_damage_rather_than_nan() {
        let mut types = DamageTypes::new();
        let template = ArmorTemplate::from_rows(
            &rows(&[
                ("DEFAULT", 1.0),
                ("FLAME", f32::NAN),
                ("FROST", f32::INFINITY),
            ]),
            &mut types,
        );
        let flame = types.lookup("FLAME").unwrap();
        let frost = types.lookup("FROST").unwrap();
        let pierce = types.intern("PIERCE").unwrap();
        assert_eq!(template.adjust_damage(flame, 100.0), 0.0);
        assert_eq!(template.adjust_damage(frost, 100.0), 0.0);
        assert_eq!(template.adjust_damage(pierce, f32::NAN), 0.0);
        assert_eq!(template.adjust_damage(pierce, f32::INFINITY), 0.0);
        assert_eq!(template.adjust_damage(pierce, -5.0), 0.0);
    }

    #[test]
    fn the_store_replaces_a_redefined_name_and_keeps_its_id() {
        let mut types = DamageTypes::new();
        let mut store = ArmorStore::new();
        let first = store
            .insert(
                "HeavyArmor",
                ArmorTemplate::from_rows(&rows(&[("DEFAULT", 0.5)]), &mut types),
            )
            .unwrap();
        let second = store
            .insert(
                "heavyarmor",
                ArmorTemplate::from_rows(&rows(&[("DEFAULT", 0.25)]), &mut types),
            )
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(store.len(), 1);
        let pierce = types.intern("PIERCE").unwrap();
        assert!((store.get(first).unwrap().coefficient(pierce) - 0.25).abs() < 1e-6);
        assert_eq!(store.id("HEAVYARMOR"), Some(first));
        assert_eq!(store.id("nothing"), None);
    }
}
