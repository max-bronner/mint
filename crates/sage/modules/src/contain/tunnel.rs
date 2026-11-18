use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use super::{boolean, milliseconds, open_contain_from_fields, OpenContain};

#[derive(Debug, Clone, PartialEq)]
pub struct TunnelContain {
    open: OpenContain,
    heal_objects: bool,
    time_for_full_heal: LogicFrames,
}

impl TunnelContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn heal_objects(&self) -> bool {
        self.heal_objects
    }

    #[must_use]
    pub const fn time_for_full_heal(&self) -> LogicFrames {
        self.time_for_full_heal
    }
}

impl Component for TunnelContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_u32(self.time_for_full_heal.get());
    }
}

impl ModuleKind for TunnelContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn tunnel_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TunnelContain, FieldError> {
    Ok(TunnelContain {
        open: open_contain_from_fields(fields)?,
        heal_objects: boolean(fields, "HealObjects")?.unwrap_or(false),
        time_for_full_heal: milliseconds(fields, "TimeForFullHeal"),
    })
}
