use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastleMemberBehavior {
    counts_for_eva_castle_breached: bool,
    being_built_sound: Option<String>,
    store_upgrade_price: bool,
    camp_destroyed_owner_eva_event: Option<String>,
    camp_destroyed_ally_eva_event: Option<String>,
    camp_destroyed_attacker_eva_event: Option<String>,
}

impl CastleMemberBehavior {
    #[must_use]
    pub const fn counts_for_eva_castle_breached(&self) -> bool {
        self.counts_for_eva_castle_breached
    }

    #[must_use]
    pub fn being_built_sound(&self) -> Option<&str> {
        self.being_built_sound.as_deref()
    }

    #[must_use]
    pub const fn store_upgrade_price(&self) -> bool {
        self.store_upgrade_price
    }

    #[must_use]
    pub fn camp_destroyed_owner_eva_event(&self) -> Option<&str> {
        self.camp_destroyed_owner_eva_event.as_deref()
    }

    #[must_use]
    pub fn camp_destroyed_ally_eva_event(&self) -> Option<&str> {
        self.camp_destroyed_ally_eva_event.as_deref()
    }

    #[must_use]
    pub fn camp_destroyed_attacker_eva_event(&self) -> Option<&str> {
        self.camp_destroyed_attacker_eva_event.as_deref()
    }
}

impl Component for CastleMemberBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.counts_for_eva_castle_breached);
        hash_optional_string(hasher, self.being_built_sound.as_deref());
        hasher.write_bool(self.store_upgrade_price);
        hash_optional_string(hasher, self.camp_destroyed_owner_eva_event.as_deref());
        hash_optional_string(hasher, self.camp_destroyed_ally_eva_event.as_deref());
        hash_optional_string(hasher, self.camp_destroyed_attacker_eva_event.as_deref());
    }
}

fn hash_optional_string(hasher: &mut StateHasher, value: Option<&str>) {
    match value {
        Some(s) => {
            hasher.write_bool(true);
            hasher.write_usize(s.len());
            hasher.write_bytes(s.as_bytes());
        }
        None => hasher.write_bool(false),
    }
}

impl ModuleKind for CastleMemberBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn castle_member_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CastleMemberBehavior, FieldError> {
    Ok(CastleMemberBehavior {
        counts_for_eva_castle_breached: optional_bool(fields, "CountsForEvaCastleBreached")?
            .unwrap_or(false),
        being_built_sound: fields.field("BeingBuiltSound").map(str::to_owned),
        store_upgrade_price: optional_bool(fields, "StoreUpgradePrice")?.unwrap_or(false),
        camp_destroyed_owner_eva_event: fields
            .field("CampDestroyedOwnerEvaEvent")
            .map(str::to_owned),
        camp_destroyed_ally_eva_event: fields.field("CampDestroyedAllyEvaEvent").map(str::to_owned),
        camp_destroyed_attacker_eva_event: fields
            .field("CampDestroyedAttackerEvaEvent")
            .map(str::to_owned),
    })
}

fn optional_bool(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    #[test]
    fn no_fields_at_all_succeeds() {
        let fields = TestFields(BTreeMap::new());
        let module = castle_member_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(!module.counts_for_eva_castle_breached());
        assert_eq!(module.being_built_sound(), None);
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            ("CountsForEvaCastleBreached", "Yes"),
            ("BeingBuiltSound", "BuildingBigConstructionLoop"),
        ]));
        let module = castle_member_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.counts_for_eva_castle_breached());
        assert_eq!(
            module.being_built_sound(),
            Some("BuildingBigConstructionLoop")
        );
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        let fields = TestFields(BTreeMap::from([("StoreUpgradePrice", "Sideways")]));
        assert!(castle_member_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }
}
