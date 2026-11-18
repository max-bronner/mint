use mint_sim::module::{Capability, CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SquishCollide;

impl Component for SquishCollide {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for SquishCollide {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Collide);
}

pub fn squish_collide_from_fields(
    _fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SquishCollide, FieldError> {
    Ok(SquishCollide)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_fields_always_succeeds_and_reads_nothing() {
        struct NoFields;
        impl ModuleFieldSource for NoFields {
            fn field(&self, _keyword: &str) -> Option<&str> {
                None
            }
        }
        assert!(squish_collide_from_fields(&NoFields, Frame::ZERO).is_ok());
    }
}
