use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StancesBehavior;

impl Component for StancesBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for StancesBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn stances_behavior_from_fields(
    _fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StancesBehavior, FieldError> {
    Ok(StancesBehavior)
}
