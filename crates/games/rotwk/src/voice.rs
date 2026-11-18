use mint_sim::{Component, Entity, Frame, StateHasher};

type SoundName = std::sync::Arc<str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VoiceKind {
    Move,
    Attack,
    Created,
    FullyCreated,
    Enter,
    DeliverRing,
    StealthOn,
    StealthOff,
    StealthBack,
    SpecialPower,
    ObjectSound,
    Impact,
    ImpactCyclonic,
}

impl VoiceKind {
    pub(crate) const fn field(self) -> &'static str {
        match self {
            Self::Move => "VoiceMove",
            Self::Attack => "VoiceAttack",
            Self::Created => "VoiceCreated",
            Self::FullyCreated => "VoiceFullyCreated",
            Self::Enter => "VoiceEnter",
            Self::DeliverRing => "VoiceDeliverRing",
            Self::StealthOn | Self::StealthBack => "SoundStealthOn",
            Self::StealthOff => "SoundStealthOff",
            Self::SpecialPower => "InitiateSound",
            Self::ObjectSound => "ObjectSound",
            Self::Impact => "SoundImpact",
            Self::ImpactCyclonic => "SoundImpactCyclonic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoiceEvent {
    pub(crate) entity: Entity,
    pub(crate) kind: VoiceKind,
    pub(crate) target: Option<Entity>,
    pub(crate) was_attacking: bool,
    pub(crate) sound: Option<SoundName>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct VoiceEvents {
    frame: Frame,
    events: Vec<VoiceEvent>,
}

impl VoiceEvents {
    pub(crate) fn push(&mut self, now: Frame, event: VoiceEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }

    pub(crate) fn of(&self, now: Frame) -> &[VoiceEvent] {
        if self.frame == now {
            &self.events
        } else {
            &[]
        }
    }
}

impl Component for VoiceEvents {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            hasher.write_u32(event.entity.index());
            hasher.write_u8(event.kind as u8);
            hasher.write_u32(event.target.map_or(u32::MAX, |target| target.index()));
            hasher.write_u8(u8::from(event.was_attacking));
            if let Some(sound) = &event.sound {
                hasher.write_bytes(sound.as_bytes());
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sim::World;

    fn entity_zero() -> Entity {
        let mut world = World::new();
        world.spawn().unwrap()
    }

    #[test]
    fn a_new_frame_replaces_the_last_frames_events() {
        let mut events = VoiceEvents::default();
        let entity = entity_zero();
        events.push(
            Frame::new(3),
            VoiceEvent {
                entity,
                kind: VoiceKind::Move,
                target: None,
                was_attacking: false,
                sound: None,
            },
        );
        events.push(
            Frame::new(3),
            VoiceEvent {
                entity,
                kind: VoiceKind::Attack,
                target: None,
                was_attacking: false,
                sound: None,
            },
        );
        assert_eq!(events.of(Frame::new(3)).len(), 2);
        assert!(events.of(Frame::new(4)).is_empty(), "frame 4 fired nothing");
        events.push(
            Frame::new(5),
            VoiceEvent {
                entity,
                kind: VoiceKind::Created,
                target: None,
                was_attacking: false,
                sound: None,
            },
        );
        assert_eq!(events.of(Frame::new(5)).len(), 1);
    }

    #[test]
    fn every_field_changes_the_hash() {
        let entity = entity_zero();
        let hash = |events: &VoiceEvents| {
            let mut hasher = StateHasher::new();
            events.hash_into(&mut hasher);
            hasher.finish()
        };
        let mut base = VoiceEvents::default();
        base.push(
            Frame::new(1),
            VoiceEvent {
                entity,
                kind: VoiceKind::Move,
                target: None,
                was_attacking: false,
                sound: None,
            },
        );
        let mut other_frame = base.clone();
        other_frame.frame = Frame::new(2);
        let mut other_kind = base.clone();
        other_kind.events[0].kind = VoiceKind::Attack;
        let mut other_target = base.clone();
        other_target.events[0].target = Some(entity);
        let mut other_was_attacking = base.clone();
        other_was_attacking.events[0].was_attacking = true;
        let mut other_sound = base.clone();
        other_sound.events[0].sound = Some(SoundName::from("SomeInitiateSound"));
        for changed in [
            other_frame,
            other_kind,
            other_target,
            other_was_attacking,
            other_sound,
        ] {
            assert_ne!(hash(&base), hash(&changed));
        }
    }
}
