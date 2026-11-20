use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Entity;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiPowerContext {
    pub health_ratio: f32,
    pub current_mode: Option<u8>,
    pub nearby_enemies: u32,
    pub nearby_allies: u32,
    pub in_combat: bool,
    pub rng_unit: f32,
    pub victim_distance: Option<f32>,
    pub weapon_is_melee: bool,
    pub victim_excluded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiPowerCandidate {
    pub entity: Entity,
    pub distance: f32,
    pub health_ratio: f32,
    pub position: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiPowerTarget {
    Object(Entity),
    Position(f32, f32),
}

pub trait AiPowerKind: Send + Sync {
    fn should_activate(&self, ctx: AiPowerContext, candidates: &[AiPowerCandidate]) -> bool;

    fn choose_target(&self, candidates: &[AiPowerCandidate]) -> Option<AiPowerTarget> {
        let _ = candidates;
        None
    }
}

#[derive(Default, Clone)]
pub struct AiPowerRegistry {
    by_name: BTreeMap<String, Arc<dyn AiPowerKind>>,
}

impl AiPowerRegistry {
    pub fn register(&mut self, name: &str, kind: Arc<dyn AiPowerKind>) {
        self.by_name.insert(name.to_ascii_uppercase(), kind);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn AiPowerKind>> {
        self.by_name.get(&name.to_ascii_uppercase())
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(&name.to_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysYes;
    impl AiPowerKind for AlwaysYes {
        fn should_activate(&self, _ctx: AiPowerContext, _candidates: &[AiPowerCandidate]) -> bool {
            true
        }
    }

    const CTX: AiPowerContext = AiPowerContext {
        health_ratio: 1.0,
        current_mode: None,
        nearby_enemies: 0,
        nearby_allies: 0,
        in_combat: false,
        rng_unit: 0.0,
        victim_distance: None,
        weapon_is_melee: false,
        victim_excluded: false,
    };

    #[test]
    fn lookup_is_case_insensitive() {
        let mut registry = AiPowerRegistry::default();
        registry.register("Example_Type", Arc::new(AlwaysYes));
        assert!(registry.get("EXAMPLE_TYPE").is_some());
        assert!(registry.get("example_type").is_some());
        assert!(registry.contains("Example_Type"));
    }

    #[test]
    fn an_unregistered_name_has_no_entry() {
        let registry = AiPowerRegistry::default();
        assert!(registry.get("NOT_REGISTERED").is_none());
        assert!(!registry.contains("NOT_REGISTERED"));
    }

    #[test]
    fn a_registered_type_answers_through_the_trait_object() {
        let mut registry = AiPowerRegistry::default();
        registry.register("YES", Arc::new(AlwaysYes));
        let kind = registry.get("YES").expect("registered");
        assert!(kind.should_activate(CTX, &[]));
    }
}
