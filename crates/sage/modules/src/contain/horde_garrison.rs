use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use super::{boolean, milliseconds, open_contain_from_fields, OpenContain};

#[derive(Debug, Clone, PartialEq)]
pub struct HordeGarrisonContain {
    open: OpenContain,
    mobile_garrison: bool,
    heal_objects: bool,
    time_for_full_heal: LogicFrames,
    immune_to_clear_building_attacks: bool,
}

impl HordeGarrisonContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn mobile_garrison(&self) -> bool {
        self.mobile_garrison
    }

    #[must_use]
    pub const fn heal_objects(&self) -> bool {
        self.heal_objects
    }

    #[must_use]
    pub const fn time_for_full_heal(&self) -> LogicFrames {
        self.time_for_full_heal
    }

    #[must_use]
    pub const fn immune_to_clear_building_attacks(&self) -> bool {
        self.immune_to_clear_building_attacks
    }
}

impl Component for HordeGarrisonContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_bool(self.heal_objects);
        hasher.write_u32(self.time_for_full_heal.get());
    }
}

impl ModuleKind for HordeGarrisonContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn horde_garrison_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<HordeGarrisonContain, FieldError> {
    Ok(HordeGarrisonContain {
        open: open_contain_from_fields(fields)?,
        mobile_garrison: boolean(fields, "MobileGarrison")?.unwrap_or(false),
        heal_objects: boolean(fields, "HealObjects")?.unwrap_or(false),
        time_for_full_heal: milliseconds(fields, "TimeForFullHeal"),
        immune_to_clear_building_attacks: boolean(fields, "ImmuneToClearBuildingAttacks")?
            .unwrap_or(false),
    })
}
