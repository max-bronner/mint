use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use super::{boolean, cash_back_percent, open_contain_from_fields, OpenContain};

#[derive(Debug, Clone, PartialEq)]
pub struct SlaughterHordeContain {
    open: OpenContain,
    cash_back_percent: f32,
    can_always_enter: bool,
}

impl SlaughterHordeContain {
    #[must_use]
    pub const fn open(&self) -> &OpenContain {
        &self.open
    }

    #[must_use]
    pub const fn cash_back_percent(&self) -> f32 {
        self.cash_back_percent
    }

    #[must_use]
    pub const fn can_always_enter(&self) -> bool {
        self.can_always_enter
    }
}

impl Component for SlaughterHordeContain {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.open.hash_into(hasher);
        hasher.write_f32(self.cash_back_percent);
        hasher.write_bool(self.can_always_enter);
    }
}

impl ModuleKind for SlaughterHordeContain {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn slaughter_horde_contain_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SlaughterHordeContain, FieldError> {
    Ok(SlaughterHordeContain {
        open: open_contain_from_fields(fields)?,
        cash_back_percent: cash_back_percent(fields)?,
        can_always_enter: boolean(fields, "CanAlwaysEnter")?.unwrap_or(false),
    })
}
