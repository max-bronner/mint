use mint_sage_combat::FxTrigger;
use mint_sim::module::{Capability, CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BurningDeath {
    pub fx: Option<String>,
}

pub fn burning_death_from_fields(
    fields: &dyn ModuleFieldSource,
) -> Result<Option<BurningDeath>, FieldError> {
    let Some(raw) = fields.field("BurningDeathBehavior") else {
        return Ok(None);
    };
    let burns = mint_sage_ini::value::parse_bool(raw)
        .ok_or_else(|| FieldError::new(format!("BurningDeathBehavior '{raw}' is not Yes/No")))?;
    Ok(burns.then(|| BurningDeath {
        fx: fields.field("BurningDeathFX").map(str::to_owned),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageCreation {
    pub list: String,
    pub trigger: FxTrigger,
    pub side: Option<DestroyedSide>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestroyedSide {
    Front,
    Right,
    Back,
    Left,
}

impl DestroyedSide {
    #[must_use]
    pub const fn model_condition(self) -> &'static str {
        match self {
            Self::Front => "DESTROYED_FRONT",
            Self::Right => "DESTROYED_RIGHT",
            Self::Back => "DESTROYED_BACK",
            Self::Left => "DESTROYED_LEFT",
        }
    }
}

fn named<T: Copy>(names: &[(&str, T)], token: &str) -> Option<T> {
    names
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, value)| *value)
}

impl DamageCreation {
    pub const EXPECTED: &'static str =
        "an ObjectCreationList, an FXTrigger and optionally a *_DESTROYED side";

    #[must_use]
    pub fn parse(args: &[String]) -> Option<Self> {
        const SIDES: [(&str, DestroyedSide); 4] = [
            ("FRONT_DESTROYED", DestroyedSide::Front),
            ("RIGHT_DESTROYED", DestroyedSide::Right),
            ("BACK_DESTROYED", DestroyedSide::Back),
            ("LEFT_DESTROYED", DestroyedSide::Left),
        ];
        let (list, trigger, side) = match args {
            [list, trigger] => (list, trigger, None),
            [list, trigger, side] => (list, trigger, Some(named(&SIDES, side)?)),
            _ => return None,
        };
        Some(Self {
            list: list.clone(),
            trigger: FxTrigger::parse(trigger)?,
            side,
        })
    }
}

fn number(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_numeric(keyword)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number")))
        .map(Some)
}

fn percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_percent(keyword)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage")))
        .map(Some)
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    mint_sage_ini::value::parse_bool(raw)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No")))
        .map(Some)
}

fn shared_body_checks(fields: &dyn ModuleFieldSource) -> Result<(), FieldError> {
    burning_death_from_fields(fields)?;
    number(fields, "CheerRadius")?;
    number(fields, "RecoveryTime")?;
    Ok(())
}

macro_rules! depletable_body {
    ($name:ident, $from_fields:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl Component for $name {
            fn hash_into(&self, _hasher: &mut StateHasher) {}
        }

        impl ModuleKind for $name {
            const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
        }

        #[doc = concat!(
                    "Builds a [`", stringify!($name), "`], registered as `\"",
                    stringify!($name), "\"` by [`crate::register_body_family`].\n\n",
                    "# Errors\n\n",
                    "[`FieldError`] if `BurningDeathBehavior` is present and not `Yes`/`No`, ",
                    "or `CheerRadius` or `RecoveryTime` is present and not a number."
                )]
        pub fn $from_fields(
            fields: &dyn ModuleFieldSource,
            _now: Frame,
        ) -> Result<$name, FieldError> {
            shared_body_checks(fields)?;
            Ok($name)
        }
    };
}

depletable_body!(
    ActiveBody,
    active_body_from_fields,
    "`ActiveBody`: an ordinary depletable-health object."
);
depletable_body!(
    StructureBody,
    structure_body_from_fields,
    "`StructureBody`: a building's health pool."
);
depletable_body!(
    ImmortalBody,
    immortal_body_from_fields,
    "`ImmortalBody`: health that damage cannot take below one point."
);
depletable_body!(
    HighlanderBody,
    highlander_body_from_fields,
    "`HighlanderBody`: health only unresistable damage can deplete."
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InactiveBody;

impl Component for InactiveBody {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for InactiveBody {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}

pub fn inactive_body_from_fields(
    _fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<InactiveBody, FieldError> {
    Ok(InactiveBody)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RespawnBody;

impl Component for RespawnBody {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for RespawnBody {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}

pub fn respawn_body_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RespawnBody, FieldError> {
    shared_body_checks(fields)?;
    boolean(fields, "CanRespawn")?;
    Ok(RespawnBody)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DelayedDeathBody;

impl Component for DelayedDeathBody {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for DelayedDeathBody {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}

pub fn delayed_death_body_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DelayedDeathBody, FieldError> {
    shared_body_checks(fields)?;
    number(fields, "DelayedDeathTime")?;
    boolean(fields, "CanRespawn")?;
    boolean(fields, "DoHealthCheck")?;
    boolean(fields, "ImmortalUntilDeathTime")?;
    Ok(DelayedDeathBody)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DetachableRiderBody;

impl Component for DetachableRiderBody {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for DetachableRiderBody {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}

pub fn detachable_rider_body_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DetachableRiderBody, FieldError> {
    shared_body_checks(fields)?;
    percent(fields, "HealthPercentageWhenRiderDies")?;
    boolean(fields, "StartsActive")?;
    Ok(DetachableRiderBody)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PorcupineFormationBodyModule;

impl Component for PorcupineFormationBodyModule {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for PorcupineFormationBodyModule {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Body);
}

pub fn porcupine_formation_body_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PorcupineFormationBodyModule, FieldError> {
    shared_body_checks(fields)?;
    number(fields, "CrusherLevelResisted")?;
    Ok(PorcupineFormationBodyModule)
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
    fn a_malformed_body_field_is_refused() {
        for (keyword, value) in [
            ("BurningDeathBehavior", "Sometimes"),
            ("CheerRadius", "near"),
            ("RecoveryTime", "soon"),
        ] {
            let fields = TestFields(BTreeMap::from([(keyword, value)]));
            assert!(
                active_body_from_fields(&fields, Frame::ZERO).is_err(),
                "{keyword} = {value} was accepted"
            );
        }
    }

    #[test]
    fn the_four_depletable_types_are_distinct_for_the_registry() {
        fn assert_module_kind<T: ModuleKind>() {}
        assert_module_kind::<ActiveBody>();
        assert_module_kind::<StructureBody>();
        assert_module_kind::<ImmortalBody>();
        assert_module_kind::<HighlanderBody>();
        assert_module_kind::<RespawnBody>();
        assert_module_kind::<DelayedDeathBody>();
        assert_module_kind::<DetachableRiderBody>();
        assert_module_kind::<PorcupineFormationBodyModule>();
    }

    #[test]
    fn inactive_body_reads_nothing_and_always_succeeds() {
        let fields = TestFields(BTreeMap::new());
        assert!(inactive_body_from_fields(&fields, Frame::ZERO).is_ok());
        let fields_with_junk = TestFields(BTreeMap::from([("MaxHealth", "not read")]));
        assert!(inactive_body_from_fields(&fields_with_junk, Frame::ZERO).is_ok());
    }

    #[test]
    fn a_malformed_field_is_refused_by_every_new_body_type() {
        let cases: [(&str, &str, &str, fn(&TestFields) -> bool); 4] = [
            ("RespawnBody", "CanRespawn", "Sometimes", |f| {
                respawn_body_from_fields(f, Frame::ZERO).is_err()
            }),
            ("DelayedDeathBody", "DelayedDeathTime", "soon", |f| {
                delayed_death_body_from_fields(f, Frame::ZERO).is_err()
            }),
            (
                "DetachableRiderBody",
                "HealthPercentageWhenRiderDies",
                "half",
                |f| detachable_rider_body_from_fields(f, Frame::ZERO).is_err(),
            ),
            (
                "PorcupineFormationBodyModule",
                "CrusherLevelResisted",
                "many",
                |f| porcupine_formation_body_from_fields(f, Frame::ZERO).is_err(),
            ),
        ];
        for (type_name, keyword, value, refused) in cases {
            let own = TestFields(BTreeMap::from([(keyword, value)]));
            assert!(
                refused(&own),
                "{type_name}: {keyword} = {value} was accepted"
            );
            let shared = TestFields(BTreeMap::from([("RecoveryTime", "soon")]));
            assert!(
                refused(&shared),
                "{type_name}: RecoveryTime = soon was accepted"
            );
        }
    }

    #[test]
    fn a_well_formed_new_body_declaration_builds() {
        let respawn = TestFields(BTreeMap::from([
            ("CheerRadius", "150"),
            ("MaxHealth", "1000"),
            ("PermanentlyKilledByFilter", "NONE"),
            ("DodgePercent", "20%"),
            ("CanRespawn", "Yes"),
        ]));
        assert!(respawn_body_from_fields(&respawn, Frame::ZERO).is_ok());

        let delayed = TestFields(BTreeMap::from([
            ("MaxHealth", "3000"),
            ("DelayedDeathTime", "999999"),
            ("CanRespawn", "No"),
            ("DoHealthCheck", "Yes"),
        ]));
        assert!(delayed_death_body_from_fields(&delayed, Frame::ZERO).is_ok());

        let rider = TestFields(BTreeMap::from([
            ("MaxHealth", "250"),
            ("MaxHealthDamaged", "125"),
            ("HealthPercentageWhenRiderDies", "50%"),
            ("StartsActive", "Yes"),
        ]));
        assert!(detachable_rider_body_from_fields(&rider, Frame::ZERO).is_ok());

        let porcupine = TestFields(BTreeMap::from([
            ("CheerRadius", "150"),
            ("MaxHealth", "500"),
            ("DamageWeaponTemplate", "PorcupineSpikes"),
            ("CrusherLevelResisted", "2"),
        ]));
        assert!(porcupine_formation_body_from_fields(&porcupine, Frame::ZERO).is_ok());
    }
}
