use mint_sim::module::{Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DestroyDie {
    mux: DieMuxData,
}

impl Component for DestroyDie {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for DestroyDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        for slot in instances.iter() {
            if slot.state.mux.gate(ctx.entity, "DestroyDie", &ctx.death) {
                ctx.destroy.request(ctx.entity);
            }
        }
    }
}

pub fn destroy_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DestroyDie, FieldError> {
    Ok(DestroyDie {
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
    fn death_requests_destruction() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<DestroyDie>(&mut world, "DestroyDie");
        let entity = spawn(&mut world);
        registry
            .attach(&mut world, entity, Frame::ZERO, DestroyDie::default())
            .unwrap();

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);
        assert_eq!(destroy.drain_sorted(), vec![entity]);
    }

    #[test]
    fn from_fields_always_succeeds() {
        struct NoFields;
        impl ModuleFieldSource for NoFields {
            fn field(&self, _keyword: &str) -> Option<&str> {
                None
            }
        }
        assert!(destroy_die_from_fields(&NoFields, Frame::ZERO).is_ok());
    }

    #[test]
    fn an_entity_with_no_declared_modules_requests_nothing() {
        let mut world = World::new();
        let registry = ModuleRegistry::new(&mut world);
        let entity = spawn(&mut world);

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);
        assert!(destroy.drain_sorted().is_empty());
    }
}
