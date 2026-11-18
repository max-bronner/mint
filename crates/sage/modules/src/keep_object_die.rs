use mint_sim::module::{Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeepObjectDie {
    mux: DieMuxData,
}

impl KeepObjectDie {
    #[must_use]
    pub fn admits(&self, death: &mint_sim::module::Death<'_>) -> bool {
        self.mux.admits(death)
    }
}

impl Component for KeepObjectDie {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for KeepObjectDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        for slot in instances.iter() {
            let _admits = slot.state.mux.gate(ctx.entity, "KeepObjectDie", &ctx.death);
        }
    }
}

pub fn keep_object_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<KeepObjectDie, FieldError> {
    Ok(KeepObjectDie {
        mux: DieMuxData::from_fields(fields)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry};
    use mint_sim::Pcg32;
    use mint_sim::{Entity, Frame, World};

    fn spawn(world: &mut World) -> Entity {
        world.spawn().unwrap()
    }

    #[test]
    fn death_requests_nothing_the_object_is_left_as_a_corpse() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<KeepObjectDie>(&mut world, "KeepObjectDie");
        let entity = spawn(&mut world);
        registry
            .attach(&mut world, entity, Frame::ZERO, KeepObjectDie::default())
            .unwrap();

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);
        assert!(
            destroy.drain_sorted().is_empty(),
            "KeepObjectDie must never queue a destroy request"
        );
    }

    #[test]
    fn from_fields_always_succeeds() {
        struct NoFields;
        impl ModuleFieldSource for NoFields {
            fn field(&self, _keyword: &str) -> Option<&str> {
                None
            }
        }
        assert!(keep_object_die_from_fields(&NoFields, Frame::ZERO).is_ok());
    }
}
