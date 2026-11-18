use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifetimeUpdate {
    expires: Frame,
}

impl LifetimeUpdate {
    #[must_use]
    pub fn new(attached_at: Frame, lifetime: LogicFrames) -> Self {
        Self {
            expires: attached_at.checked_add(lifetime).unwrap_or(Frame::FOREVER),
        }
    }

    #[must_use]
    pub const fn expires(self) -> Frame {
        self.expires
    }
}

impl Component for LifetimeUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.expires.hash_into(hasher);
    }
}

impl ModuleKind for LifetimeUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if ctx.now >= self.expires {
            ctx.destroy.request(ctx.entity);
            return Sleep::Forever;
        }
        match self.expires.since(ctx.now) {
            Some(remaining) => Sleep::Frames(remaining),
            None => Sleep::Forever,
        }
    }
}

pub fn lifetime_update_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> Result<LifetimeUpdate, FieldError> {
    let ms = fields
        .field_numeric("MaxLifetime")
        .ok_or_else(|| FieldError::new("MaxLifetime is missing or not a number/known macro"))?;
    if ms < 0.0 {
        return Err(FieldError::new(format!("MaxLifetime '{ms}' is negative")));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lifetime = crate::special_ability_update::retail_duration_ticks(ms as u32);
    Ok(LifetimeUpdate::new(now, lifetime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry};
    use mint_sim::Pcg32;
    use mint_sim::{Entity, World};

    fn spawn(world: &mut World) -> Entity {
        world.spawn().unwrap()
    }

    #[test]
    fn expiry_is_relative_to_the_frame_it_was_attached_on() {
        let module = LifetimeUpdate::new(Frame::new(10), LogicFrames::from_frames(20));
        assert_eq!(module.expires(), Frame::new(30));
    }

    #[test]
    fn it_wakes_exactly_once_at_expiry_and_never_before() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<LifetimeUpdate>(&mut world, "LifetimeUpdate");
        let entity = spawn(&mut world);
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                LifetimeUpdate::new(Frame::ZERO, LogicFrames::from_frames(10)),
            )
            .unwrap();

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        for frame in 0..10 {
            registry.update(&mut world, Frame::new(frame), &mut rng, &mut destroy);
            assert!(
                destroy.drain_sorted().is_empty(),
                "must not request destruction before frame 10, saw it at frame {frame}"
            );
        }
        registry.update(&mut world, Frame::new(10), &mut rng, &mut destroy);
        assert_eq!(destroy.drain_sorted(), vec![entity]);
    }
}
